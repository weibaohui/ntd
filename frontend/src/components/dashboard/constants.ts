export const TIME_RANGE_OPTIONS: { label: string; value: number | 'custom' }[] = [
  { label: '5小时', value: 5 },
  { label: '7天', value: 168 },
  { label: '14天', value: 336 },
  { label: '30天', value: 720 },
  { label: '自定义', value: 'custom' },
];

export const STATUS_COLORS: Record<string, string> = {
  pending: '#94a3b8',
  running: '#3b82f6',
  completed: '#22c55e',
  failed: '#ef4444',
};

const EMPTY_STATE_QUOTES = [
  '心若如镜，来者皆照，去者不留。',
  '万念俱息处，真心自现前。',
  '不取一尘，万象皆净。',
  '心若无形，何处不自在。',
  '念起如风，觉知如山。',
  '心若无声，万法皆听。',
  '不求不拒，方得本真。',
  '心若无界，一念通天。',
  '万象皆幻，唯觉不动。',
  '心若无痕，事事皆轻。',
  '不住于相，方见诸相空。',
  '心若无执，处处皆圆满。',
  '念起如潮，觉照如岸。',
  '心若无阴，光自无尽。',
  '不随念走，念自归寂。',
  '心若无缚，万法皆通。',
  '不逐一念，一念自灭。',
  '心若无碍，步步皆通途。',
  '不守一境，一境皆自在。',
  '心若无偏，万物皆平等。',
  '不求圆满，圆满自来。',
  '心若无重，万事皆轻盈。',
  '不逐前尘，前尘如烟散。',
  '心若无形，形形皆自在。',
  '不住当下，当下自明。',
  '心若无我，万法皆一。',
  '不守成见，见见皆新。',
  '心若无畏，天地皆宽。',
  '不执善恶，善恶皆空花。',
  '心若无欲，万物皆清凉。',
  '不随喜怒，喜怒皆幻影。',
  '心若无乱，万象皆秩序。',
  '不求远方，远方在心间。',
  '心若无界，步步皆无边。',
  '不逐光影，光影自分明。',
  '心若无名，万法皆可名。',
  '不守一念，一念皆虚空。',
  '心若无住，处处皆安然。',
  '不求悟道，道已随行。',
  '心若无尘，风来不动。',
  '不逐旧梦，旧梦自消散。',
  '心若无声，万籁皆寂然。',
  '不求真相，真相自显露。',
  '心若无求，所求皆得。',
  '不执成败，成败皆如露。',
  '心若无苦，苦亦成空。',
  '不逐未来，未来自来。',
  '心若无边，念念皆无尽。',
  '不守过往，过往皆如烟。',
];

export const RANDOM_QUOTE = EMPTY_STATE_QUOTES[Math.floor(Math.random() * EMPTY_STATE_QUOTES.length)];

export const STATUS_LABELS: Record<string, string> = {
  pending: '待处理',
  running: '运行中',
  completed: '已完成',
  failed: '失败',
};

export const TRIGGER_LABELS: Record<string, string> = {
  manual: '手动',
  cron: '定时',
  slash_command: '命令',
  default_response: '默认回复',
};

export const TRIGGER_COLORS: Record<string, string> = {
  manual: '#3b82f6',
  cron: '#8b5cf6',
  slash_command: '#f59e0b',
  default_response: '#22c55e',
};

export const MODEL_COLORS = ['#8b5cf6', '#3b82f6', '#22c55e', '#f59e0b', '#ef4444', '#0891b2', '#ec4899', '#6366f1'];

export const ACTIVE_TASKS_MIN_HEIGHT = 148;
