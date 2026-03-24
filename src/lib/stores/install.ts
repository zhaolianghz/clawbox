import { writable } from 'svelte/store';

export interface SystemCheck {
  nodejs: boolean;
  openclaw: boolean;
  network: 'cn' | 'global' | 'unknown';
}

export interface InstallProgress {
  step: 'checking' | 'terms' | 'installing' | 'complete';
  progress: number;
  log: string[];
  error?: string;
}

export const systemCheck = writable<SystemCheck>({
  nodejs: false,
  openclaw: false,
  network: 'unknown'
});

export const installProgress = writable<InstallProgress>({
  step: 'checking',
  progress: 0,
  log: []
});

export const needsInstall = writable(false);
export const installComplete = writable(false);

export function resetInstall() {
  systemCheck.set({
    nodejs: false,
    openclaw: false,
    network: 'unknown'
  });
  installProgress.set({
    step: 'checking',
    progress: 0,
    log: []
  });
  needsInstall.set(false);
  installComplete.set(false);
}
