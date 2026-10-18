/** Smallest 1/2/5x10^n value that still contains the data.
 *
 *  Charts that carry percentages fit a fixed 0-100 scale, but token rates do
 *  not: clipping decode throughput at 100 tok/s flattened every fast model
 *  into a straight line while the figures beside it reported the true peak.
 *  A data-driven ceiling keeps the shape honest without inventing axis labels. */
export function niceCeiling(values: number[]): number {
  // Non-finite readings must not widen the scale into NaN geometry: an Infinity
  // peak would make every point collapse to `value / Infinity` and draw nothing.
  const peak = values.reduce(
    (highest, value) => (Number.isFinite(value) && value > highest ? value : highest),
    0,
  );
  if (!(peak > 0)) return 100;
  const magnitude = 10 ** Math.floor(Math.log10(peak));
  const unit = peak / magnitude;
  const step = unit <= 1 ? 1 : unit <= 2 ? 2 : unit <= 5 ? 5 : 10;
  return step * magnitude;
}
