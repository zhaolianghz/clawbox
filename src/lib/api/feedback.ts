import { invoke } from '@tauri-apps/api/core';

export interface Feedback {
  id: string;
  category: string;
  message: string;
  contact: string | null;
  created_at: number;
}

export async function feedback_submit(
  category: string,
  message: string,
  contact?: string
): Promise<Feedback> {
  return await invoke<Feedback>('feedback_submit', { category, message, contact: contact ?? null });
}

export async function feedback_list(): Promise<Feedback[]> {
  try {
    return await invoke<Feedback[]>('feedback_list');
  } catch {
    return [];
  }
}
