export type DesktopRequestToken = {
  requestId: number;
  modeGeneration: number;
};

export type DesktopRequestTracker = {
  nextRequestId: number;
  activeRequestId: number;
  modeGeneration: number;
};

export function createDesktopRequestTracker(): DesktopRequestTracker {
  return {
    nextRequestId: 1,
    activeRequestId: 0,
    modeGeneration: 0,
  };
}

export function beginDesktopRequest(tracker: DesktopRequestTracker): DesktopRequestToken {
  const token: DesktopRequestToken = {
    requestId: tracker.nextRequestId,
    modeGeneration: tracker.modeGeneration,
  };
  tracker.nextRequestId += 1;
  tracker.activeRequestId = token.requestId;
  return token;
}

export function invalidateDesktopRequestsForModeChange(tracker: DesktopRequestTracker): void {
  tracker.modeGeneration += 1;
  tracker.activeRequestId = 0;
}

export function isDesktopRequestCurrent(
  tracker: DesktopRequestTracker,
  token: DesktopRequestToken,
): boolean {
  return (
    tracker.activeRequestId === token.requestId &&
    tracker.modeGeneration === token.modeGeneration
  );
}
