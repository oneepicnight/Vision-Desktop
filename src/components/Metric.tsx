import type { ReactNode } from "react";

type MetricProps = {
  label: string;
  value: ReactNode;
};

export function Metric({ label, value }: MetricProps) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
