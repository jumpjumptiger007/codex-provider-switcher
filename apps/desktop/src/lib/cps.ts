import { invoke } from "@tauri-apps/api/core";
import type { Provider } from "../types/provider";
import type { Status } from "../types/provider";

export function getProviders(): Promise<Provider[]> {
  return invoke<Provider[]>("get_providers");
}

export interface DesktopError {
  code: string;
  message: string;
}

export function getStatus(): Promise<Status> {
  return invoke<Status>("get_status").catch((reason: unknown) => {
    if (isDesktopError(reason)) throw reason;
    throw {
      code: "internal_error",
      message: "Current setup is temporarily unavailable.",
    } satisfies DesktopError;
  });
}

function isDesktopError(value: unknown): value is DesktopError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    typeof value.code === "string" &&
    "message" in value &&
    typeof value.message === "string"
  );
}
