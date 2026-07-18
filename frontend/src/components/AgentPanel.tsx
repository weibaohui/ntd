// Agent Tab 视图：从日志中识别派生的子 agent，展示其输入(prompt)与输出(result)。
//
// 与后端 agent_progress 同口径（按 tool_name 识别 spawn 工具调用），但这里直接渲染
// execution_logs 里的原文——因为 execution_records.agent_runs 按用户要求「3不存」只存元数据，
// prompt/result 原文留在日志里，由本组件就近展示。
//
// 输出配对说明：用「向后最近的 tool_result」做尽力配对。对 mimo 族（task/actor 工具的
// tool_use 事件自带 output，结果紧随）准确；对 claudecode（子 agent 内部工具调用穿插在主流）
// 仅近似——故输出块标注「参考」，用户可去对话视图核对。

import { Tag, Empty } from 'antd';
import type { LogEntry } from '@/types';

/** 派生子 agent 的工具名（小写）；与后端 AGENT_TOOL_NAMES 保持一致。 */
const AGENT_TOOL_NAMES = ['agent', 'task', 'actor', 'spawn_agent'];

interface AgentInfo {
  key: string;
  name: string;
  role?: string;
  input?: string;
  output?: string;
  timestamp: string;
}

function isAgentTool(name?: string): boolean {
  if (!name) return false;
  return AGENT_TOOL_NAMES.includes(name.toLowerCase());
}

function asString(v: unknown): string | undefined {
  return typeof v === 'string' ? v : undefined;
}

/** 从 toolInputJson 解析 name/role；与后端 pick_str 同口径（含 mimo 的 operation 下沉）。 */
function parseAgentMeta(toolInputJson?: string): { name?: string; role?: string; input?: string } {
  if (!toolInputJson) return {};
  let v: unknown;
  try {
    v = JSON.parse(toolInputJson);
  } catch {
    // 不是合法 JSON 时，把整段原文当输入展示，至少让用户看到 prompt。
    return { input: toolInputJson };
  }
  const root = typeof v === 'object' && v !== null ? (v as Record<string, unknown>) : {};
  // mimo 族把真实入参包在 operation 里，先下沉一层。
  const src = (root.operation && typeof root.operation === 'object'
    ? (root.operation as Record<string, unknown>)
    : root);
  const name = asString(src.description) ?? asString(src.name) ?? asString(src.title) ?? asString(src.subject);
  const role = asString(src.subagent_type) ?? asString(src.agent_type) ?? asString(src.type) ?? asString(src.role);
  return { name, role, input: toolInputJson };
}

/** 遍历日志，收集每个 agent spawn 的元信息 + 邻近输出。 */
function collectAgents(logs: LogEntry[]): AgentInfo[] {
  const agents: AgentInfo[] = [];
  logs.forEach((log, idx) => {
    if (log.type !== 'tool_call' || !isAgentTool(log.toolName ?? log.content)) return;
    const meta = parseAgentMeta(log.toolInputJson);
    if (!meta.name) return; // 拿不到名字的不算一个可展示的 agent
    agents.push({
      key: `${idx}-${meta.name}`,
      name: meta.name,
      role: meta.role,
      input: meta.input,
      output: findNearestOutput(logs, idx),
      timestamp: log.timestamp,
    });
  });
  return agents;
}

/** 向后找最近的 tool_result 作为该 agent 的输出；遇到下一个 agent spawn 即停。 */
function findNearestOutput(logs: LogEntry[], from: number): string | undefined {
  for (let j = from + 1; j < logs.length; j++) {
    const l = logs[j];
    if (l.type === 'tool_result') return l.content;
    if (l.type === 'tool_call' && isAgentTool(l.toolName ?? l.content)) break;
  }
  return undefined;
}

export function AgentPanel({ logs }: { logs: LogEntry[] }) {
  const agents = collectAgents(logs);
  if (agents.length === 0) {
    return <Empty description="未识别到子 Agent" image={Empty.PRESENTED_IMAGE_SIMPLE} />;
  }
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      {agents.map((a) => (
        <div
          key={a.key}
          style={{ border: '1px solid var(--color-border-light)', borderRadius: 8, padding: 10 }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6, flexWrap: 'wrap' }}>
            <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--color-text)' }}>{a.name}</span>
            {a.role && (
              <Tag color="blue" style={{ margin: 0, fontSize: 11 }}>
                {a.role}
              </Tag>
            )}
          </div>
          {a.input && <PreBlock label="输入" text={a.input} />}
          {a.output && <PreBlock label="输出（邻近结果·参考）" text={a.output} />}
        </div>
      ))}
    </div>
  );
}

function PreBlock({ label, text }: { label: string; text: string }) {
  return (
    <div style={{ marginBottom: 6 }}>
      <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 2 }}>{label}</div>
      <pre
        style={{
          margin: 0,
          background: 'var(--log-bg)',
          color: 'var(--log-text)',
          padding: '6px 8px',
          borderRadius: 6,
          fontSize: 11,
          fontFamily: 'var(--font-mono)',
          whiteSpace: 'pre-wrap',
          wordBreak: 'break-all',
          maxHeight: 240,
          overflow: 'auto',
        }}
      >
        {text}
      </pre>
    </div>
  );
}
