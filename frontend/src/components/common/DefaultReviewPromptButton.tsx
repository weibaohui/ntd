// DefaultReviewPromptButton — 评审 prompt「使用默认值」按钮。
//
// 点击从后端拉取系统内置 DEFAULT_REVIEWER_PROMPT 常量，经 onApply 回调填入调用方的输入框。
// 抽成共享组件：环节评审 prompt（LinkPropertyForm）与全局评审模板编辑（ReviewTemplatesPanel）复用，
// 避免两处重复 fetch + 错误处理逻辑。loading 期间禁用防重复点击；失败弹 message.error 不阻断调用方。

import { useState } from 'react';
import { Button, message } from 'antd';
import { DownloadOutlined } from '@ant-design/icons';
import { getDefaultReviewPrompt } from '@/utils/database/reviewTemplates';

interface DefaultReviewPromptButtonProps {
  /** 拿到默认 prompt 文本后的填入回调，由调用方决定写到哪个字段（直接覆盖原内容）。 */
  onApply: (text: string) => void;
  /** 按钮文案，默认「使用默认值」。 */
  label?: string;
}

/**
 * 评审 prompt「使用默认值」按钮。
 *
 * 处理流程：点击 → getDefaultReviewPrompt() → onApply(text) + 成功提示；
 * 失败 → message.error，按钮恢复可点供重试。
 */
export function DefaultReviewPromptButton({
  onApply,
  label = '使用默认值',
}: DefaultReviewPromptButtonProps) {
  const [loading, setLoading] = useState(false);

  const handleClick = async () => {
    setLoading(true);
    try {
      const text = await getDefaultReviewPrompt();
      onApply(text);
      message.success('已填入默认评审模板');
    } catch (err) {
      message.error(`获取默认模板失败：${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <Button size="small" type="link" icon={<DownloadOutlined />} loading={loading} onClick={handleClick}>
      {label}
    </Button>
  );
}
