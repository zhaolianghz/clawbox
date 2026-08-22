import { invoke } from '@tauri-apps/api/core';

/** ok | warn | error | info — drives icon & color in the report panel. */
export type CheckStatus = 'ok' | 'warn' | 'error' | 'info';

export interface DoctorCheck {
  id: string;
  /** Backend fallback title; the UI prefers i18n key `agents.doctor.<id>.title`. */
  title: string;
  status: CheckStatus;
  /** Dynamic detail (missing deps, drifted agent ids, …), shown verbatim. */
  detail: string;
  hint: string | null;
}

export interface DoctorReport {
  checks: DoctorCheck[];
  /** RFC3339, shown verbatim. */
  ran_at: string;
}

export function doctor_run(): Promise<DoctorReport> {
  return invoke<DoctorReport>('doctor_run');
}
