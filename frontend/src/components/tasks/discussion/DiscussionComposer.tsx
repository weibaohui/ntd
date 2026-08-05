// 讨论区输入器：Markdown 编辑 + inline @选择器 + 发送（M4）。
// 输入 @ 即在编辑器上方弹出候选（专家优先 + 执行器，按已输入文本过滤），点击插入 @<规范名>。
// 后端按正文里的 @token 解析触发执行；这里只负责便捷、准确地插入规范名。

import { useEffect, useState } from 'react';
import { Button, Space, Typography } from 'antd';
import { SendOutlined } from '@ant-design/icons';
import { MdEditor } from '@/components/MdEditor';
import { EXECUTORS_FOR_PICKER } from '@/utils/executors';
import { getAllExperts } from '@/utils/database/experts';
import { getExpertDisplayName } from '@/types/expert';
import type { ExpertMetadata } from '@/types/expert';

const { Text } = Typography;

interface DiscussionComposerProps {
  value: string;
  onChange: (v: string) => void;
  onSend: () => void;
  sending: boolean;
  /** 回复目标提示（如 "#3 张三"）；null=主楼层新帖。 */
  replyTo?: string | null;
  onCancelReply?: () => void;
}

/** 候选项：专家或执行器，统一用 name（规范名，插入正文 + 后端匹配）。 */
interface MentionCandidate {
  kind: 'expert' | 'executor';
  /** 规范名：专家 expert.name；执行器 executor.value。 */
  name: string;
  /** 展示名。 */
  display: string;
}

/** 候选浮层每组（专家/执行器）最多展示条数，避免列表过长。 */
const MAX_PER_GROUP = 4;

/**
 * 检测正文末尾正在输入的 @token：末尾 `@` 后跟非空白/非@ 字符。
 * 返回 query（@ 之后的文本，空串=刚输入@）；无匹配返回 null。
 * 纯函数，只覆盖「在末尾输入 @」的常见场景；中间插入不触发（MVP 取舍）。
 */
function detectAtToken(value: string): { query: string } | null {
  const m = value.match(/@([^\s@]*)$/);
  return m ? { query: m[1] } : null;
}

/**
 * 按 query 过滤专家 + 执行器，构造候选列表（专家优先，与后端 resolve_mentions 消歧顺序一致）。
 * query 为空各取前 MAX_PER_GROUP；非空则按规范名/展示名包含 query 过滤。
 */
function buildCandidates(query: string, experts: ExpertMetadata[]): MentionCandidate[] {
  const q = query.trim().toLowerCase();
  // 命中条件：无 query（刚输入@）全显；否则规范名或展示名包含 query。
  const hit = (norm: string, disp: string) => !q || norm.toLowerCase().includes(q) || disp.toLowerCase().includes(q);
  const exps: MentionCandidate[] = experts
    .filter((ex) => hit(ex.name, getExpertDisplayName(ex)))
    .slice(0, MAX_PER_GROUP)
    .map((ex) => ({ kind: 'expert', name: ex.name, display: getExpertDisplayName(ex) }));
  const execs: MentionCandidate[] = EXECUTORS_FOR_PICKER
    .filter((e) => hit(e.value, e.label))
    .slice(0, MAX_PER_GROUP)
    .map((e) => ({ kind: 'executor', name: e.value, display: e.label }));
  return [...exps, ...execs];
}

interface MentionsPickerProps {
  candidates: MentionCandidate[];
  onPick: (name: string) => void;
}

/** 候选浮层：渲染在编辑器上方（absolute bottom:100%），点击选中插入。 */
function MentionsPicker({ candidates, onPick }: MentionsPickerProps) {
  return (
    <div
      style={{
        position: 'absolute', bottom: '100%', left: 0, right: 0, marginBottom: 2,
        background: 'var(--color-bg, #fff)', border: '1px solid var(--color-border, #d9d9d9)',
        borderRadius: 6, maxHeight: 200, overflowY: 'auto', zIndex: 20,
        boxShadow: '0 2px 8px rgba(0,0,0,0.12)',
      }}
    >
      {candidates.map((c) => (
        <div
          key={`${c.kind}:${c.name}`}
          role="option"
          onClick={() => onPick(c.name)}
          style={{ padding: '6px 12px', cursor: 'pointer', display: 'flex', alignItems: 'center', gap: 8 }}
          onMouseEnter={(e) => { e.currentTarget.style.background = 'var(--color-bg-hover, #f5f5f5)'; }}
          onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent'; }}
        >
          <Text style={{ fontSize: 12, color: c.kind === 'expert' ? '#1677ff' : '#52c41a' }}>
            {c.kind === 'expert' ? '专家' : '执行器'}
          </Text>
          <Text>{c.display}</Text>
        </div>
      ))}
    </div>
  );
}

export function DiscussionComposer({
  value, onChange, onSend, sending, replyTo, onCancelReply,
}: DiscussionComposerProps) {
  // 专家列表几乎不变，挂载时拉一次（执行器是静态常量，无需拉取）。
  const [experts, setExperts] = useState<ExpertMetadata[]>([]);
  useEffect(() => {
    // 无专家时 @专家 候选为空，不影响 @执行器，静默失败。
    getAllExperts().then(setExperts).catch(() => {});
  }, []);

  // 末尾 @ 检测 → 候选；无候选（含无 @）时不显示浮层。
  const atToken = detectAtToken(value);
  const candidates = atToken ? buildCandidates(atToken.query, experts) : [];
  const showPicker = atToken !== null && candidates.length > 0;

  /** 选中候选：把末尾的 @query 替换为 @<规范名> + 空格，避免后续输入并入名字。 */
  const insertMention = (name: string) => {
    onChange(value.replace(/@([^\s@]*)$/, `@${name} `));
  };

  const canSend = value.trim().length > 0 && !sending;

  return (
    <div>
      {replyTo ? (
        <div style={{ marginBottom: 8 }}>
          <Text type="secondary">回复 {replyTo} </Text>
          <Button size="small" type="link" onClick={onCancelReply}>取消</Button>
        </div>
      ) : null}
      <div style={{ position: 'relative' }}>
        {showPicker ? <MentionsPicker candidates={candidates} onPick={insertMention} /> : null}
        <MdEditor value={value} onChange={onChange} height={140} />
      </div>
      <Space style={{ marginTop: 8, width: '100%', justifyContent: 'space-between', flexWrap: 'wrap' }}>
        <Text type="secondary" style={{ fontSize: 12 }}>
          输入 @ 触发选择专家/执行器，如 @前端架构师、@codex
        </Text>
        <Button type="primary" icon={<SendOutlined />} disabled={!canSend} loading={sending} onClick={onSend}>
          发送
        </Button>
      </Space>
    </div>
  );
}
