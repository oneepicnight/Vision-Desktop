export type AppAction = (name: string, fn: () => Promise<unknown>) => Promise<void>;
