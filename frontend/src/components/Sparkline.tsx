interface Props {
  values: number[];
  color: string;
  max?: number;
  width?: number;
  height?: number;
}

export function Sparkline({ values, color, max, width = 64, height = 20 }: Props) {
  if (values.length < 2) {
    return <svg width={width} height={height} className="spark" />;
  }
  const peak = max ?? Math.max(1, ...values);
  const n = values.length;
  const step = width / (n - 1);
  const pts = values
    .map((v, i) => {
      const x = i * step;
      const y = height - Math.min(1, v / peak) * (height - 2) - 1;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  const last = values[n - 1];
  const lastY = height - Math.min(1, last / peak) * (height - 2) - 1;
  return (
    <svg width={width} height={height} className="spark" preserveAspectRatio="none">
      <polyline points={pts} fill="none" stroke={color} strokeWidth={1.5} />
      <circle cx={width} cy={lastY} r={1.6} fill={color} />
    </svg>
  );
}
