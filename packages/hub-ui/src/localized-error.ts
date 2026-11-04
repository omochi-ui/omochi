import type { TFunction } from 'i18next';
import { SessionFault } from '@omochi-ui/core/browser';
import { BoardFault } from './board-repository.js';
import { SharingFault } from './sharing-repository.js';

/** Translate public error codes at presentation time, preserving the original fault. */
export function localizedError(error: unknown, t: TFunction): string {
  if (error instanceof BoardFault) return t(`boardError.${error.code}`);
  if (error instanceof SessionFault) return t(`sessionError.${error.code}`);
  if (error instanceof SharingFault) return t(`share.${error.code}`);
  return error instanceof Error ? error.message : t('boardError.StorageUnavailable');
}
