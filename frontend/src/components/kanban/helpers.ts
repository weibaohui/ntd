import type { Todo } from '../../types';
import { COLUMNS } from './constants';

export function getColumnForStatus(status: Todo['status']): ColumnDef {
  return COLUMNS.find(c => c.status === status) || COLUMNS[0];
}

import type { ColumnDef } from './constants';
