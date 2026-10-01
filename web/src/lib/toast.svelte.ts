export interface Toast {
  id: number;
  kind: 'ok' | 'error' | 'info';
  text: string;
}

export const toasts = $state({ list: [] as Toast[] });
let next = 1;

export function toast(text: string, kind: Toast['kind'] = 'ok', ms = 5000): void {
  const id = next++;
  toasts.list.push({ id, kind, text });
  setTimeout(() => dismiss(id), ms);
}

export function dismiss(id: number): void {
  toasts.list = toasts.list.filter((t) => t.id !== id);
}
