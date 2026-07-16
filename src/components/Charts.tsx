import { useId } from 'react';
import type { MetricTone } from '../types';

const toneColors: Record<MetricTone, string> = {
  cyan: '#58d9ff',
  violet: '#9c8cff',
  mint: '#63e6be',
  amber: '#ffbd5b',
  rose: '#ff7190',
};

interface LineChartProps {
  values: number[];
  tone?: MetricTone;
  height?: number;
  compact?: boolean;
}
export function LineChart({ values, tone = 'cyan', height = 120, compact = false }: LineChartProps) {
  const reactId = useId().replace(/:/g, '');
  const width = 520;
  const verticalPadding = compact ? 8 : 14;
  const points = values
    .map((value, index) => {
      const x = (index / Math.max(values.length - 1, 1)) * width;
      const y = height - verticalPadding - (value / 100) * (height - verticalPadding * 2);
      return `${x},${y}`;
    })
    .join(' ');
  const areaPoints = `0,${height} ${points} ${width},${height}`;
  const color = toneColors[tone];

  return (
    <svg
      className="line-chart"
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      role="img"
      aria-label="Live performance history"
    >
      <defs>
        <linearGradient id={`fill-${reactId}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={color} stopOpacity="0.26" />
          <stop offset="100%" stopColor={color} stopOpacity="0" />
        </linearGradient>
        <linearGradient id={`stroke-${reactId}`} x1="0" y1="0" x2="1" y2="0">
          <stop offset="0%" stopColor={color} stopOpacity="0.45" />
          <stop offset="100%" stopColor={color} />
        </linearGradient>
      </defs>
      {!compact && (
        <g className="chart-grid">
          <line x1="0" x2={width} y1={height * 0.25} y2={height * 0.25} />
          <line x1="0" x2={width} y1={height * 0.5} y2={height * 0.5} />
          <line x1="0" x2={width} y1={height * 0.75} y2={height * 0.75} />
        </g>
      )}
      <polygon points={areaPoints} fill={`url(#fill-${reactId})`} />
      <polyline
        className="chart-line"
        points={points}
        fill="none"
        stroke={`url(#stroke-${reactId})`}
        strokeWidth={compact ? 2.2 : 2.6}
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

interface DonutProps {
  value: number;
  tone: MetricTone;
  label: string;
  size?: 'small' | 'large';
}

export function Donut({ value, tone, label, size = 'small' }: DonutProps) {
  return (
    <div
      className={`donut donut-${size}`}
      style={
        {
          '--donut-value': `${Math.round(value * 3.6)}deg`,
          '--donut-color': toneColors[tone],
        } as React.CSSProperties
      }
      role="img"
      aria-label={`${label}: ${Math.round(value)} percent`}
    >
      <div className="donut-inner">
        <strong>{Math.round(value)}</strong>
        <span>%</span>
      </div>
    </div>
  );
}
