use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Minimal, fuzz-safe GGUF header reader.
///
/// Reads only the header, KV metadata and tensor table — never tensor data —
/// with hard caps on every allocation so a corrupt or hostile file cannot
/// exhaust memory. GGUF value types: 0 u8, 1 i8, 2 u16, 3 i16, 4 u32, 5 i32,
/// 6 f32, 7 bool, 8 string, 9 array, 10 u64, 11 i64, 12 f64.
#[derive(Clone, Debug, Default)]
pub struct GgufMeta {
    pub architecture: Option<String>,
    pub parameter_count: Option<u64>,
    pub block_count: Option<u64>,
    pub head_count: Option<u64>,
    /// Average KV heads per active layer. Single-value GQA and per-layer
    /// arrays (hybrid attention, where 0 marks linear-attention layers) both
    /// reduce to one defensible number; zero entries are excluded.
    pub head_count_kv: Option<f64>,
    pub key_length: Option<u64>,
    pub value_length: Option<u64>,
    pub embedding_length: Option<u64>,
    pub context_length: Option<u64>,
    pub expert_count: Option<u64>,
    pub expert_used_count: Option<u64>,
    /// GGML file type id (quantization), when reported.
    pub file_type: Option<u64>,
    /// Exact weight bytes summed from the tensor table.
    pub tensor_bytes: Option<u64>,
}

const MAX_STRING_BYTES: u64 = 1 << 20;
const MAX_KV_PAIRS: u64 = 4_096;
const MAX_TENSORS: u64 = 16_384;
const MAX_ARRAY_ELEMENTS: u64 = 4_194_304;

struct Reader<R: Read> {
    inner: R,
    remaining: u64,
}

impl<R: Read> Reader<R> {
    fn new(inner: R, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
        }
    }

    fn take(&mut self, count: usize) -> Option<Vec<u8>> {
        if self.remaining < count as u64 {
            return None;
        }
        let mut buffer = vec![0_u8; count];
        self.inner.read_exact(&mut buffer).ok()?;
        self.remaining -= count as u64;
        Some(buffer)
    }

    fn scalar(&mut self, bytes: usize) -> Option<u64> {
        let raw = self.take(bytes)?;
        let mut value = 0_u64;
        for (index, byte) in raw.iter().enumerate() {
            value |= (*byte as u64) << (8 * index);
        }
        Some(value)
    }

    fn string(&mut self) -> Option<String> {
        let length = self.scalar(8)?;
        if length > MAX_STRING_BYTES {
            return None;
        }
        String::from_utf8(self.take(length as usize)?).ok()
    }
}

fn element_width(value_type: u32) -> Option<u64> {
    match value_type {
        0 | 1 | 7 => Some(1),
        2 | 3 => Some(2),
        4..=6 => Some(4),
        10..=12 => Some(8),
        _ => None,
    }
}

enum Value {
    Unsigned(u64),
    Signed(i64),
    Text(String),
    /// Small numeric arrays only (per-layer head counts, rope sections).
    Array(Vec<u64>),
}

const MAX_USEFUL_ARRAY_ELEMENTS: usize = 1_024;

fn read_value<R: Read>(reader: &mut Reader<R>, value_type: u32) -> Option<Value> {
    Some(match value_type {
        0 => Value::Unsigned(reader.scalar(1)?),
        1 => Value::Signed(reader.scalar(1)? as u8 as i8 as i64),
        2 => Value::Unsigned(reader.scalar(2)?),
        3 => Value::Signed(reader.scalar(2)? as u16 as i16 as i64),
        4 => Value::Unsigned(reader.scalar(4)?),
        5 => Value::Signed(reader.scalar(4)? as u32 as i32 as i64),
        6 => {
            reader.take(4)?;
            Value::Unsigned(0) // float scalars are not used by the advisor
        }
        7 => Value::Unsigned(reader.scalar(1)?),
        8 => Value::Text(reader.string()?),
        9 => {
            let element_type = reader.scalar(4)? as u32;
            let count = reader.scalar(8)?;
            if count > MAX_ARRAY_ELEMENTS {
                return None;
            }
            let mut values = Vec::new();
            for _ in 0..count {
                if element_type == 8 {
                    reader.string()?;
                    continue;
                }
                match element_type {
                    0 | 7 => values.push(reader.scalar(1)?),
                    1 => values.push(reader.scalar(1)? as u8 as i8 as i64 as u64),
                    2 => values.push(reader.scalar(2)?),
                    3 => values.push(reader.scalar(2)? as u16 as i16 as i64 as u64),
                    4 => values.push(reader.scalar(4)?),
                    5 => values.push(reader.scalar(4)? as u32 as i32 as i64 as u64),
                    6 | 12 => {
                        reader.take(element_width(element_type)? as usize)?;
                        values.push(0);
                    }
                    10 => values.push(reader.scalar(8)?),
                    11 => values.push(reader.scalar(8)? as i64 as u64),
                    _ => return None,
                }
                if values.len() >= MAX_USEFUL_ARRAY_ELEMENTS {
                    values.clear();
                }
            }
            Value::Array(values)
        }
        10 => Value::Unsigned(reader.scalar(8)?),
        11 => Value::Signed(reader.scalar(8)? as i64),
        _ => return None,
    })
}

/// `(block_size, block_bytes)` per GGML type id, mirroring the
/// `ggml_type_traits` table in ggml-common.h. Type ids are persisted in GGUF
/// files, so they follow the historical enum (F32=0, Q4_0=2, Q4_K=12, F64=28,
/// BF16=30), not the declaration order of any source array.
fn ggml_block(type_id: u32) -> Option<(u64, u64)> {
    Some(match type_id {
        0 => (1, 4),      // F32
        1 => (1, 2),      // F16
        2 => (32, 18),    // Q4_0
        3 => (32, 20),    // Q4_1
        6 => (32, 22),    // Q5_0
        7 => (32, 24),    // Q5_1
        8 => (32, 34),    // Q8_0
        9 => (32, 36),    // Q8_1
        10 => (256, 84),  // Q2_K
        11 => (256, 110), // Q3_K
        12 => (256, 144), // Q4_K
        13 => (256, 176), // Q5_K
        14 => (256, 210), // Q6_K
        15 => (256, 292), // Q8_K
        16 => (256, 66),  // IQ2_XXS
        17 => (256, 74),  // IQ2_XS
        18 => (256, 98),  // IQ3_XXS
        19 => (256, 50),  // IQ1_S
        20 => (32, 18),   // IQ4_NL
        21 => (256, 110), // IQ3_S
        22 => (256, 82),  // IQ2_S
        23 => (256, 136), // IQ4_XS
        24 => (1, 1),     // I8
        25 => (1, 2),     // I16
        26 => (1, 4),     // I32
        27 => (1, 8),     // I64
        28 => (1, 8),     // F64
        29 => (256, 56),  // IQ1_M
        30 => (1, 2),     // BF16
        34 => (256, 54),  // TQ1_0
        35 => (256, 66),  // TQ2_0
        39 => (32, 17),   // MXFP4
        40 => (64, 36),   // NVFP4
        41 => (128, 18),  // Q1_0
        42 => (64, 18),   // Q2_0
        // 4, 5, 31-33, 36-38 were removed from the format; unknown ids must
        // return None so callers fall back instead of guessing.
        _ => return None,
    })
}

/// `ggml_row_size()`: exact weight bytes for `elements` values of `type_id`.
fn ggml_row_size(type_id: u32, elements: u64) -> Option<u64> {
    let (block, block_bytes) = ggml_block(type_id)?;
    if elements == 0 {
        return Some(0);
    }
    Some(elements.div_ceil(block).saturating_mul(block_bytes))
}

pub fn probe(path: &Path) -> Option<GgufMeta> {
    let mut file = File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    if size < 32 {
        return None;
    }
    let mut reader = Reader::new(&mut file, size.min(64 * 1024 * 1024));
    if reader.take(4)?.as_slice() != b"GGUF" {
        return None;
    }
    let _version = reader.scalar(4)?;
    let tensor_count = reader.scalar(8)?;
    let kv_count = reader.scalar(8)?;
    if kv_count > MAX_KV_PAIRS || tensor_count > MAX_TENSORS {
        return None;
    }

    let mut meta = GgufMeta::default();
    let mut kv_seen = 0_u64;
    for _ in 0..kv_count {
        let key = match reader.string() {
            Some(key) => key,
            None => break,
        };
        let value_type = match reader.scalar(4) {
            Some(value) => value as u32,
            None => break,
        };
        let value = match read_value(&mut reader, value_type) {
            Some(value) => value,
            None => break,
        };
        kv_seen += 1;
        apply_kv(&mut meta, &key, value);
    }
    if kv_seen == 0 {
        return None;
    }

    // Tensor table: name, dims, type, offset — sum exact byte sizes.
    let mut tensor_bytes = 0_u64;
    let mut counted = 0_u64;
    for _ in 0..tensor_count {
        if reader.string().is_none() {
            break;
        }
        let dims = match reader.scalar(4) {
            Some(value) => value,
            None => break,
        };
        if dims > 8 {
            break;
        }
        let mut elements = 1_u64;
        let mut overflow = false;
        let mut stream_exhausted = false;
        for _ in 0..dims {
            // A truncated dimension stream would desync every later read, so
            // stop the whole tensor table rather than parsing garbage.
            let Some(extent) = reader.scalar(8) else {
                stream_exhausted = true;
                break;
            };
            elements = match elements.checked_mul(extent) {
                Some(value) => value,
                None => {
                    overflow = true;
                    0
                }
            };
        }
        if stream_exhausted {
            break;
        }
        let type_id = match reader.scalar(4) {
            Some(value) => value as u32,
            None => break,
        };
        if reader.scalar(8).is_none() {
            break;
        }
        counted += 1;
        if overflow {
            continue;
        }
        if let Some(row_bytes) = ggml_row_size(type_id, elements) {
            tensor_bytes = tensor_bytes.saturating_add(row_bytes);
        }
    }
    if counted > 0 {
        meta.tensor_bytes = Some(tensor_bytes);
    }
    Some(meta)
}

fn apply_kv(meta: &mut GgufMeta, key: &str, value: Value) {
    match value {
        Value::Unsigned(number) => apply_unsigned(meta, key, number),
        Value::Signed(number) => {
            if number >= 0 {
                apply_unsigned(meta, key, number as u64);
            }
        }
        Value::Text(text) => {
            if key == "general.architecture" {
                meta.architecture = Some(text);
            }
        }
        Value::Array(numbers) => {
            if !key.ends_with("attention.head_count_kv") {
                return;
            }
            let active: Vec<u64> = numbers.into_iter().filter(|value| *value > 0).collect();
            if active.is_empty() {
                return;
            }
            let sum: u64 = active
                .iter()
                .fold(0_u64, |acc, value| acc.saturating_add(*value));
            meta.head_count_kv = Some(sum as f64 / active.len() as f64);
        }
    }
}

fn apply_unsigned(meta: &mut GgufMeta, key: &str, number: u64) {
    let suffix = key.rsplit('.').next().unwrap_or(key);
    match suffix {
        "parameter_count" => meta.parameter_count = Some(number),
        "block_count" => meta.block_count = Some(number),
        "head_count" => {
            if key.ends_with("attention.head_count") {
                meta.head_count = Some(number);
            }
        }
        "head_count_kv" => meta.head_count_kv = Some(number as f64),
        "key_length" => meta.key_length = Some(number),
        "value_length" => {
            if key.ends_with("attention.value_length") {
                meta.value_length = Some(number);
            }
        }
        "embedding_length" => meta.embedding_length = Some(number),
        "context_length" => {
            if !key.contains("vision") {
                meta.context_length = Some(number);
            }
        }
        "expert_count" => meta.expert_count = Some(number),
        "expert_used_count" => meta.expert_used_count = Some(number),
        "file_type" if key.starts_with("general.") => {
            meta.file_type = Some(number);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_gguf(name: &str, header: &[u8]) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("visor-gguf-{name}-{}.bin", std::process::id()));
        let mut file = File::create(&path).expect("temp file");
        file.write_all(b"GGUF").unwrap();
        file.write_all(&3_u32.to_le_bytes()).unwrap();
        file.write_all(header).unwrap();
        path
    }

    fn push_string(out: &mut Vec<u8>, text: &str) {
        out.extend_from_slice(&(text.len() as u64).to_le_bytes());
        out.extend_from_slice(text.as_bytes());
    }

    fn kv_u32(out: &mut Vec<u8>, key: &str, value: u32) {
        push_string(out, key);
        out.extend_from_slice(&4_u32.to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn kv_u64(out: &mut Vec<u8>, key: &str, value: u64) {
        push_string(out, key);
        out.extend_from_slice(&10_u32.to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn reads_realistic_qwen3_header() {
        let mut body: Vec<u8> = Vec::new();
        body.extend_from_slice(&2_u64.to_le_bytes()); // tensor count
        body.extend_from_slice(&8_u64.to_le_bytes()); // kv count
        push_string(&mut body, "general.architecture");
        body.extend_from_slice(&8_u32.to_le_bytes());
        push_string(&mut body, "qwen3");
        kv_u64(&mut body, "general.parameter_count", 9_653_104_368);
        kv_u32(&mut body, "qwen3.block_count", 32);
        kv_u32(&mut body, "qwen3.attention.head_count", 16);
        push_string(&mut body, "qwen3.attention.head_count_kv");
        body.extend_from_slice(&9_u32.to_le_bytes());
        body.extend_from_slice(&4_u32.to_le_bytes()); // u32 elements
        body.extend_from_slice(&2_u64.to_le_bytes());
        body.extend_from_slice(&4_u32.to_le_bytes());
        body.extend_from_slice(&8_u32.to_le_bytes());
        kv_u32(&mut body, "qwen3.context_length", 262_144);
        kv_u32(&mut body, "qwen3.expert_count", 128);
        kv_u32(&mut body, "qwen3.expert_used_count", 8);

        // Tensor table: one F16 4096×4096 tensor → 32 MiB.
        push_string(&mut body, "blk.0.attn_q.weight");
        body.extend_from_slice(&2_u32.to_le_bytes());
        body.extend_from_slice(&4096_u64.to_le_bytes());
        body.extend_from_slice(&4096_u64.to_le_bytes());
        body.extend_from_slice(&1_u32.to_le_bytes());
        body.extend_from_slice(&0_u64.to_le_bytes());

        let path = temp_gguf("real", &body);
        let meta = probe(&path).expect("parses header");
        std::fs::remove_file(&path).ok();

        assert_eq!(meta.architecture.as_deref(), Some("qwen3"));
        assert_eq!(meta.parameter_count, Some(9_653_104_368));
        assert_eq!(meta.block_count, Some(32));
        assert_eq!(meta.head_count, Some(16));
        assert_eq!(meta.head_count_kv, Some(6.0));
        assert_eq!(meta.context_length, Some(262_144));
        assert_eq!(meta.expert_count, Some(128));
        assert_eq!(meta.expert_used_count, Some(8));
        assert_eq!(meta.tensor_bytes, Some(4096_u64 * 4096 * 2));
    }

    #[test]
    fn rejects_non_gguf_and_garbage_lengths() {
        let path = temp_gguf("garbage", &{
            let mut body: Vec<u8> = Vec::new();
            body.extend_from_slice(&0_u64.to_le_bytes());
            body.extend_from_slice(&1_u64.to_le_bytes());
            push_string(&mut body, "general.architecture");
            body.extend_from_slice(&8_u32.to_le_bytes());
            // Absurd string length must abort, not allocate.
            body.extend_from_slice(&u64::MAX.to_le_bytes());
            body
        });
        assert!(probe(&path).is_none());
        std::fs::remove_file(&path).ok();
    }

    /// Live probe of a real Ollama blob on this machine. Ignored by default:
    /// run with `cargo test live_ -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_real_ollama_blob_parses_exact_metadata() {
        let Some(local) = std::env::var_os("LOCALAPPDATA") else {
            return;
        };
        // Any recent server.log proves Ollama is installed here; find blobs via models dir env or default.
        let models = std::env::var_os("OLLAMA_MODELS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(&local)
                    .join("Ollama")
                    .join("models")
            });
        let Ok(entries) = std::fs::read_dir(models.join("blobs")) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.len() < 500 * 1024 * 1024 {
                continue; // only model-weight blobs carry GGUF headers
            }
            if let Some(meta) = probe(&path) {
                println!(
                    "live GGUF {}: {meta:?}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
                assert!(meta.parameter_count.is_some() || meta.tensor_bytes.is_some());
                return;
            }
        }
        println!("no GGUF blob probed on this machine");
    }

    #[test]
    fn row_sizes_match_published_bits_per_weight() {
        // Guard the id -> (block, bytes) table: these ids are persisted in
        // GGUF files and were previously mapped to the wrong quant families.
        assert_eq!(ggml_row_size(0, 1024), Some(4096)); // F32
        assert_eq!(ggml_row_size(1, 1024), Some(2048)); // F16
        assert_eq!(ggml_row_size(2, 256), Some(144)); // Q4_0, 4.5 bpw
        assert_eq!(ggml_row_size(8, 256), Some(272)); // Q8_0
        assert_eq!(ggml_row_size(10, 256), Some(84)); // Q2_K
        assert_eq!(ggml_row_size(11, 256), Some(110)); // Q3_K
        assert_eq!(ggml_row_size(12, 256), Some(144)); // Q4_K
        assert_eq!(ggml_row_size(13, 256), Some(176)); // Q5_K
        assert_eq!(ggml_row_size(14, 256), Some(210)); // Q6_K
        assert_eq!(ggml_row_size(20, 32), Some(18)); // IQ4_NL
        assert_eq!(ggml_row_size(22, 256), Some(82)); // IQ2_S
        assert_eq!(ggml_row_size(28, 16), Some(128)); // F64, not BF16
        assert_eq!(ggml_row_size(30, 1024), Some(2048)); // BF16
                                                         // Partial blocks round up; removed and unknown ids yield no estimate.
        assert_eq!(ggml_row_size(12, 300), Some(288));
        assert_eq!(ggml_row_size(4, 256), None);
        assert_eq!(ggml_row_size(99, 256), None);
    }

    #[test]
    fn q4_k_tensor_bytes_use_exact_block_math() {
        let mut body: Vec<u8> = Vec::new();
        body.extend_from_slice(&1_u64.to_le_bytes()); // one tensor
        body.extend_from_slice(&1_u64.to_le_bytes()); // one kv pair
        push_string(&mut body, "general.architecture");
        body.extend_from_slice(&8_u32.to_le_bytes());
        push_string(&mut body, "qwen2");
        push_string(&mut body, "blk.0.attn_q.weight");
        body.extend_from_slice(&1_u32.to_le_bytes()); // one dimension
        body.extend_from_slice(&1024_u64.to_le_bytes());
        body.extend_from_slice(&12_u32.to_le_bytes()); // GGML_TYPE_Q4_K
        body.extend_from_slice(&0_u64.to_le_bytes());

        let path = temp_gguf("q4k", &body);
        let meta = probe(&path).expect("parses header");
        std::fs::remove_file(&path).ok();

        // 1024 weights = 4 blocks of 256 x 144 bytes. The previous table
        // assumed ~1 byte per weight and reported 1024.
        assert_eq!(meta.tensor_bytes, Some(576));
    }

    #[test]
    fn huge_tokenizer_array_does_not_allocate() {
        let mut body: Vec<u8> = Vec::new();
        body.extend_from_slice(&0_u64.to_le_bytes());
        body.extend_from_slice(&1_u64.to_le_bytes());
        push_string(&mut body, "tokenizer.ggml.merges");
        body.extend_from_slice(&9_u32.to_le_bytes());
        body.extend_from_slice(&8_u32.to_le_bytes()); // string elements
        body.extend_from_slice(&300_000_u64.to_le_bytes()); // huge count
        push_string(&mut body, "a");
        push_string(&mut body, "b");
        let path = temp_gguf("trunc", &body);
        // Truncated stream → None, no panic, no giant allocation.
        assert!(probe(&path).is_none());
        std::fs::remove_file(&path).ok();
    }
}
