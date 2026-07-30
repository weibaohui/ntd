// DefaultReviewPromptButton 单元测试。
// 覆盖：点击拉取默认 prompt → 以文本回调 onApply；拉取失败 → onApply 不触发。
// mock 掉评审模板 API 客户端，避免真实网络请求；断言聚焦 onApply 是否被正确文本调用。

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

// mock 评审模板 API 客户端：每个用例在 beforeEach 后指定返回值，互不干扰。
vi.mock('@/utils/database/reviewTemplates', () => ({
  getDefaultReviewPrompt: vi.fn(),
}));

import { getDefaultReviewPrompt } from '@/utils/database/reviewTemplates';
import { DefaultReviewPromptButton } from './DefaultReviewPromptButton';

describe('DefaultReviewPromptButton', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('点击后拉取默认 prompt 并以文本回调 onApply', async () => {
    // 模拟后端返回 DEFAULT_REVIEWER_PROMPT 常量文本。
    vi.mocked(getDefaultReviewPrompt).mockResolvedValue('默认评审 prompt 正文');
    const onApply = vi.fn();
    render(<DefaultReviewPromptButton onApply={onApply} />);
    fireEvent.click(screen.getByText('使用默认值'));
    // async handleClick 走完 fetch + onApply 链。
    await waitFor(() => {
      expect(onApply).toHaveBeenCalledWith('默认评审 prompt 正文');
    });
  });

  it('拉取失败时 onApply 不被触发（避免把空/错误内容写进输入框）', async () => {
    vi.mocked(getDefaultReviewPrompt).mockRejectedValue(new Error('网络错误'));
    const onApply = vi.fn();
    render(<DefaultReviewPromptButton onApply={onApply} />);
    fireEvent.click(screen.getByText('使用默认值'));
    // 确认失败分支已执行（API 被调用），但 onApply 未触发。
    await waitFor(() => {
      expect(getDefaultReviewPrompt).toHaveBeenCalled();
    });
    expect(onApply).not.toHaveBeenCalled();
  });
});
