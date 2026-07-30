type StatusLineProps = {
  message: string;
};

export function StatusLine({ message }: StatusLineProps) {
  return <div className="status-line">{message}</div>;
}
