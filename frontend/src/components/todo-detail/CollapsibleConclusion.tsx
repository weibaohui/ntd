/**
 * 共享的「可折叠结论」组件
 *
 * 取代 RecordDetailView / ChainGroupCard / NarrowHistoryCard 中重复出现的
 * 结论展示区（带 XMarkdown 渲染 + 复制按钮）。
 *
 * 设计要点：
 * 1. 默认展开 — 保持与历史行为一致；用户折叠后用 localStorage 记住偏好。
 * 2. 折叠态只保留头部（标题 / 字数 / 复制 / 切换按钮），Markdown 内容
 *    整体不渲染，避免大段内容占据屏幕，符合 issue #652 的诉求。
 * 3. 切换按钮带 ARIA 属性，便于无障碍与 Playwright 选择器定位。
 * 4. messageApi 是可选的：调用方（如 RecordDetailView）若想用动态 message
 *    实例，传入即可；否则回落到 antd 的静态 message。
 *
 * 对应 issue：#652 「todo执行历史页面 结论 显示区做成可折叠的效果」
 */

import { useEffect, useRef, useState } from 'react';
import { Button, message as antdMessage } from 'antd';
import type { MessageInstance } from 'antd/es/message/interface';
import { CaretDownOutlined, CaretUpOutlined, CopyOutlined } from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import { copyToClipboard } from '@/utils/clipboard';

// 折叠状态在 localStorage 中的 key 前缀；按 recordId 区分避免互相串扰
const STORAGE_KEY_PREFIX = 'ntd:conclusion-collapsed:';

// 切换按钮与正文之间的间距，避免切换后边框贴在一起
const HEADER_MARGIN = 4;
const HEADER_GAP = 8;

// 折叠态下，整块被压缩到只剩一行头部；展开态保留 marginBottom 与内层 Markdown 容器
const CONTAINER_MARGIN_BOTTOM = 12;

export interface CollapsibleConclusionProps {
  /** Markdown 文本，来自 ExecutionRecord.result */
  result: string;
  /** 执行状态，仅用于切换 success / failed 背景色 */
  status: string;
  /** 动态 message 实例；不传则使用 antd 静态 message */
  messageApi?: MessageInstance;
  /** 是否在头部显示「结论」二字标题；目前仅 RecordDetailView 启用 */
  showTitle?: boolean;
  /** 记录 ID；提供时折叠状态会按 ID 持久化到 localStorage */
  recordId?: number | string;
}

/**
 * 从 localStorage 读取折叠状态；无值时返回 undefined，由调用方决定默认值。
 * 把读取单独抽出来以便在 effect 中复用。
 */
function readCollapsedState(key: string | null): boolean | undefined {
  if (!key) return undefined;
  try {
    const stored = window.localStorage.getItem(key);
    if (stored === null) return undefined;
    return stored === 'true';
  } catch {
    // 隐私模式或 storage 配额耗尽时 read 可能抛错，按"未持久化"处理
    return undefined;
  }
}

/**
 * 写入折叠状态；同样吞掉异常，避免破坏 UI 交互。
 */
function writeCollapsedState(key: string | null, collapsed: boolean): void {
  if (!key) return;
  try {
    window.localStorage.setItem(key, String(collapsed));
  } catch {
    // 写入失败（隐私模式 / 配额）静默吞掉；下次刷新会回到默认展开
  }
}

export function CollapsibleConclusion({
  result,
  status,
  messageApi,
  showTitle = false,
  recordId,
}: CollapsibleConclusionProps) {
  // 按 recordId 派生 storage key；recordId 缺失则不持久化
  const storageKey = recordId !== undefined && recordId !== null
    ? `${STORAGE_KEY_PREFIX}${recordId}`
    : null;

  // 初始状态：未持久化时默认展开（向后兼容）；有持久化值时尊重用户选择
  const [collapsed, setCollapsed] = useState<boolean>(() => {
    const stored = readCollapsedState(storageKey);
    return stored ?? false;
  });

  // recordId 变化时（如切换到另一条执行记录）重新读取持久化值，
  // 让每条记录各自的折叠状态独立保持。首次挂载由 useState lazy init 处理，
  // 用 isFirst 跳过首次 effect 调用避免一次多余的 no-op 重渲染。
  const isFirstMountRef = useRef(true);
  useEffect(() => {
    if (isFirstMountRef.current) {
      isFirstMountRef.current = false;
      return;
    }
    const stored = readCollapsedState(storageKey);
    // 显式区分 "未持久化" 与 "持久化为 false"：前者用默认 false，后者用持久化值
    setCollapsed(stored ?? false);
  }, [storageKey]);

  // 切换折叠态：写本地状态 + 持久化到 localStorage
  const toggle = () => {
    setCollapsed(prev => {
      const next = !prev;
      writeCollapsedState(storageKey, next);
      return next;
    });
  };

  // 复制 Markdown 源文到剪贴板；统一走 copyToClipboard，兼容 HTTP 环境
  const handleCopy = async () => {
    const api = messageApi ?? antdMessage;
    try {
      const ok = await copyToClipboard(result || '');
      api[ok ? 'success' : 'error'](ok ? '已复制到剪贴板' : '复制失败');
    } catch {
      api.error('复制失败');
    }
  };

  // 与原实现保持一致：成功用绿底，失败用红底。
  // 保留旧行为：running / pending / cancelled 等非 success 状态
  // 暂走 failed 视觉（历史包袱，未在本次 PR 中调整）。
  const statusClass = status === 'success'
    ? 'history-result-success'
    : 'history-result-failed';

  return (
    <div
      className={`history-result ${statusClass} ${collapsed ? 'history-result-collapsed' : 'history-result-expanded'}`}
      style={{ marginBottom: CONTAINER_MARGIN_BOTTOM }}
      data-testid="collapsible-conclusion"
      data-collapsed={collapsed ? 'true' : 'false'}
    >
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: collapsed ? 0 : HEADER_MARGIN,
          gap: HEADER_GAP,
        }}
      >
        <Button
          type="text"
          size="small"
          onClick={toggle}
          icon={collapsed ? <CaretDownOutlined /> : <CaretUpOutlined />}
          aria-expanded={!collapsed}
          aria-label={collapsed ? '展开结论' : '折叠结论'}
          data-testid="conclusion-toggle"
          style={{ display: 'inline-flex', alignItems: 'center', gap: 4, padding: '0 8px' }}
        >
          {showTitle && (
            <span
              style={{
                fontSize: 13,
                fontWeight: 600,
                color: 'var(--color-text)',
                marginRight: 4,
              }}
            >
              结论
            </span>
          )}
          <span
            style={{
              fontSize: 11,
              color: 'var(--color-text-tertiary)',
              fontWeight: 500,
            }}
          >
            {/* 用 spread 数 code points 而非 UTF-16 code units，
                避免一个 emoji 被算成 2 字让字数虚高。 */}
            {[...result].length} 字
          </span>
        </Button>
        <Button
          type="text"
          size="small"
          icon={<CopyOutlined />}
          onClick={handleCopy}
          aria-label="复制结论"
          data-testid="conclusion-copy"
        />
      </div>
      {!collapsed && (
        <div className="conclusion-content" data-testid="conclusion-content">
          <XMarkdown content={result} />
        </div>
      )}
    </div>
  );
}
