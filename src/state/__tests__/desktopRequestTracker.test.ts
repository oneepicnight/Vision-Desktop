import {
  beginDesktopRequest,
  createDesktopRequestTracker,
  invalidateDesktopRequestsForModeChange,
  isDesktopRequestCurrent,
} from "../desktopRequestTracker";

function assertEqual<T>(actual: T, expected: T, message?: string) {
  if (actual !== expected) {
    throw new Error(message ?? `Expected ${String(expected)}, got ${String(actual)}`);
  }
}

export function runDesktopRequestTrackerTests() {
  {
    const tracker = createDesktopRequestTracker();
    const first = beginDesktopRequest(tracker);
    const second = beginDesktopRequest(tracker);

    assertEqual(isDesktopRequestCurrent(tracker, first), false, "older request should be stale after newer request starts");
    assertEqual(isDesktopRequestCurrent(tracker, second), true, "newest request should remain current");
  }

  {
    const tracker = createDesktopRequestTracker();
    const active = beginDesktopRequest(tracker);
    invalidateDesktopRequestsForModeChange(tracker);

    assertEqual(isDesktopRequestCurrent(tracker, active), false, "mode change should invalidate active request");

    const afterModeChange = beginDesktopRequest(tracker);
    assertEqual(isDesktopRequestCurrent(tracker, afterModeChange), true, "request started after mode change should be current");
  }

  {
    const tracker = createDesktopRequestTracker();
    const actionToken = beginDesktopRequest(tracker);

    assertEqual(isDesktopRequestCurrent(tracker, actionToken), true, "action token should remain current before nested refresh");
    assertEqual(isDesktopRequestCurrent(tracker, actionToken), true, "nested refresh using the same token should remain current");

    const pollingToken = beginDesktopRequest(tracker);
    assertEqual(isDesktopRequestCurrent(tracker, actionToken), false, "new polling request should supersede older action token");
    assertEqual(isDesktopRequestCurrent(tracker, pollingToken), true, "latest polling request should be current");
  }

  {
    const tracker = createDesktopRequestTracker();
    const request = beginDesktopRequest(tracker);

    assertEqual(isDesktopRequestCurrent(tracker, request), true, "unrelated state changes must not invalidate request tracking");
  }
}

runDesktopRequestTrackerTests();
