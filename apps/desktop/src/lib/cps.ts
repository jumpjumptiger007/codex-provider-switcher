import { invoke } from "@tauri-apps/api/core";
import type { CredentialStatus, Model, Provider, Status } from "../types/provider";

export interface DesktopError {
  code: string;
  message: string;
}

export function getProviders(): Promise<Provider[]> {
  return invokeDesktop<Provider[]>("get_providers");
}

export function getStatus(): Promise<Status> {
  return invokeDesktop<Status>("get_status");
}

export function getCredentialStatus(providerId: string): Promise<CredentialStatus> {
  return invokeDesktop<CredentialStatus>("get_credential_status", { providerId });
}

export function saveCredential(
  providerId: string,
  credential: string,
): Promise<CredentialStatus> {
  return invokeDesktop<CredentialStatus>("save_credential", {
    providerId,
    credential,
  });
}

export function getModels(providerId: string): Promise<Model[]> {
  return invokeDesktop<Model[]>("get_models", { providerId });
}

export function switchModel(providerId: string, modelId: string): Promise<Status> {
  return invokeDesktop<Status>("switch_model", { providerId, modelId });
}

function invokeDesktop<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args).catch((reason: unknown) => {
    if (isDesktopError(reason)) throw reason;
    throw {
      code: "internal_error",
      message: "This operation could not be completed. Try again.",
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
