/**
 * 复制按钮组件
 *
 * 采用同步 document.execCommand('copy') 方式复制文本，
 * 不依赖异步 navigator.clipboard.writeText，不依赖 clipboard.js 事件绑定。
 *
 * 关键样式参数参考 clipboard.js 内部 createFakeElement 的实现：
 * - position:absolute + left:-9999px 移到屏幕外（保持 textarea 自然尺寸，浏览器可选中）
 * - opacity:0 视觉隐藏但保持 DOM 尺寸正常
 * - readonly 防止移动端弹出键盘
 * - focus() + select() 确保选区被浏览器认可
 *
 * 用法：
 * ```tsx
 * <CopyButton text="要复制的内容" onCopy={() => message.success('已复制')}>
 *   复制
 * </CopyButton>
 * ```
 */
import { useRef, useState, forwardRef, useCallback } from 'react';
import { Button } from 'antd';
import type { ButtonProps } from 'antd';
import { CopyOutlined, CheckOutlined } from '@ant-design/icons';

export interface CopyButtonProps extends Omit<ButtonProps, 'onClick' | 'children'> {
  /** 要复制的文本 */
  text: string;
  /** 复制成功回调 */
  onCopy?: () => void;
  /** 按钮内容 */
  children?: React.ReactNode;
  /** 复制成功后是否短暂显示勾号（默认 true） */
  showFeedback?: boolean;
  /** 反馈持续时间（ms，默认 2000） */
  feedbackDuration?: number;
}

export const CopyButton = forwardRef<HTMLButtonElement | HTMLAnchorElement, CopyButtonProps>(function CopyButton({
  text,
  onCopy,
  children = '复制',
  showFeedback = true,
  feedbackDuration = 2000,
  icon = <CopyOutlined />,
  ...rest
}, ref) {
  const [copied, setCopied] = useState(false);
  const onCopyRef = useRef(onCopy);
  const feedbackTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 始终保持最新的 onCopy 引用
  onCopyRef.current = onCopy;

  // 复制成功后的通用反馈逻辑：更新图标状态、触发 onCopy 回调、
  // 启动定时器在反馈持续时间后恢复图标。
  const showCopiedFeedback = useCallback(() => {
    setCopied(true);
    onCopyRef.current?.();
    if (showFeedback) {
      if (feedbackTimerRef.current) clearTimeout(feedbackTimerRef.current);
      feedbackTimerRef.current = setTimeout(() => {
        setCopied(false);
        feedbackTimerRef.current = null;
      }, feedbackDuration);
    }
  }, [showFeedback, feedbackDuration]);

  // 点击处理：创建临时 textarea 并同步 execCommand('copy')。
  // 样式完全复制 clipboard.js 内部 createFakeElement 的实现：
  // - position:absolute + left:-9999px 移到屏幕外（而非 width/height=0）
  // - opacity:0 视觉隐藏但 DOM 尺寸正常
  // - readonly 防止移动端弹出键盘
  // - 显式 focus + select 确保选区被浏览器认可
  const handleClick = useCallback(() => {
    const textarea = document.createElement('textarea');
    textarea.value = text;
    // 以下样式完全复制 clipboard.js 的 createFakeElement 实现
    textarea.style.position = 'absolute';
    textarea.style.left = '-9999px';
    textarea.style.top = '0px';
    textarea.style.opacity = '0';
    textarea.style.fontSize = '12pt';
    textarea.style.border = '0';
    textarea.style.padding = '0';
    textarea.style.margin = '0';
    textarea.style.overflow = 'hidden';
    textarea.setAttribute('readonly', '');

    document.body.appendChild(textarea);
    // 部分浏览器要求 textarea 有 focus 后 select 才被认可
    textarea.focus();
    textarea.select();

    let ok = false;
    try {
      ok = document.execCommand('copy');
    } catch {
      // execCommand 在不支持的环境中抛异常
    }

    document.body.removeChild(textarea);

    if (ok) {
      showCopiedFeedback();
    }
  }, [text, showCopiedFeedback]);

  return (
    <Button
      ref={(node) => {
        if (typeof ref === 'function') ref(node);
        else if (ref) ref.current = node;
      }}
      icon={copied ? <CheckOutlined /> : icon}
      onClick={handleClick}
      {...rest}
    >
      {children}
    </Button>
  );
});