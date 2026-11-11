export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object") {
    const message = Reflect.get(error, "message");
    if (typeof message === "string") return message;
    try {
      return JSON.stringify(error);
    } catch {
      // Fall through to the safest generic representation.
    }
  }
  return String(error);
}
