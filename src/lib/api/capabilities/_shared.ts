import type { BackendId } from '../backends';

export interface BackendError {
  backend: BackendId;
  message: string;
}

export interface TaggedItem<T> {
  backend: BackendId;
  item: T;
}

export interface TaggedListResult<T> {
  items: TaggedItem<T>[];
  errors: BackendError[];
}