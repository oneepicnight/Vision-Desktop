import React from "react";

export function usePollingEffect(callback: () => Promise<void>, intervalMs: number) {
  React.useEffect(() => {
    callback();
    const id = window.setInterval(callback, intervalMs);
    return () => window.clearInterval(id);
  }, [callback, intervalMs]);
}
