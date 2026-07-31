type StatusLineProps = {
  message: string;
};

export function StatusLine({ message }: StatusLineProps) {
  return (
    <div className="status-line" role="status">
      <span className="status-line-dot" aria-hidden="true" />
      <span>{message}</span>
    </div>
  );
}
