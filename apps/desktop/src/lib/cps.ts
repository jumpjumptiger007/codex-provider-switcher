import { invoke } from "@tauri-apps/api/core";
import type { Provider } from "../types/provider";

export function getProviders(): Promise<Provider[]> {
  return invoke<Provider[]>("get_providers");
}
