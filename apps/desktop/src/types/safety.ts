export type DoctorSeverity = "ok" | "info" | "warning" | "error";

export interface DoctorFinding {
  check: string;
  severity: DoctorSeverity;
  message: string;
}

export interface DoctorReport {
  findings: DoctorFinding[];
  hasErrors: boolean;
}

export interface RestoreResult {
  restoredFrom: string;
  recoveryBackup: string;
}
