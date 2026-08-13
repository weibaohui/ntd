import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useSkillCatalog } from './useSkillCatalog';

// 屏蔽 antd App.useApp 的 message 依赖（hook 内 loader 的错误提示路径）
vi.mock('antd', async (importOriginal) => {
  const actual = await importOriginal<typeof import('antd')>();
  return {
    ...actual,
    App: { ...actual.App, useApp: () => ({ message: { error: vi.fn(), success: vi.fn(), warning: vi.fn() } }) },
  };
});

// mock bundledApi：loader 的数据源（本测试聚焦视图切换的联动重置编排，不验证请求参数）
vi.mock('@/api/bundled', () => ({
  bundledApi: {
    getSkills: vi.fn().mockResolvedValue({ skills: [], sources: {}, total: 0 }),
    getSkillSources: vi.fn().mockResolvedValue({ sources: [], total: 0 }),
  },
}));

// useIsMobile 不影响本测试，但 hook 未用到——无需 mock（仅主组件用）

describe('useSkillCatalog 视图切换联动重置', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('初始态：browse-sources 视图、全部页码为 1', async () => {
    const { result } = renderHook(() => useSkillCatalog());
    expect(result.current.viewMode).toBe('browse-sources');
    expect(result.current.activeSource).toBeNull();
    expect(result.current.browseSourcesPage).toBe(1);
    expect(result.current.browseSkillsPage).toBe(1);
    expect(result.current.allPage).toBe(1);
  });

  it('switchToAllSkills：重置 activeSource/来源筛选/搜索词/allPage 回 1', async () => {
    const { result } = renderHook(() => useSkillCatalog());
    // 先制造非默认状态
    act(() => {
      result.current.setSearchText('claude');
      result.current.setFilterSource('claude');
      result.current.setAllPage(5);
      result.current.enterSource('claude');
    });
    expect(result.current.allPage).toBe(5);

    act(() => {
      result.current.switchToAllSkills();
    });
    expect(result.current.viewMode).toBe('all-skills');
    expect(result.current.activeSource).toBeNull();
    expect(result.current.filterSource).toBe('all');
    expect(result.current.searchText).toBe('');
    expect(result.current.allPage).toBe(1);
  });

  it('switchToSourceBrowse：重置 activeSource/搜索词/两个浏览页码回 1', async () => {
    const { result } = renderHook(() => useSkillCatalog());
    act(() => {
      result.current.setSearchText('x');
      result.current.setBrowseSourcesPage(4);
      result.current.setBrowseSkillsPage(3);
    });
    act(() => {
      result.current.enterSource('openclaw');
    });
    expect(result.current.browseSkillsPage).toBe(1); // enterSource 重置

    act(() => {
      result.current.setBrowseSourcesPage(7);
      result.current.setSearchText('y');
    });
    act(() => {
      result.current.switchToSourceBrowse();
    });
    expect(result.current.viewMode).toBe('browse-sources');
    expect(result.current.activeSource).toBeNull();
    expect(result.current.searchText).toBe('');
    expect(result.current.browseSourcesPage).toBe(1);
    expect(result.current.browseSkillsPage).toBe(1);
  });

  it('enterSource：设置 activeSource 并重置技能页码与搜索词（防空白页鬼畜）', () => {
    const { result } = renderHook(() => useSkillCatalog());
    act(() => {
      result.current.setBrowseSkillsPage(10);
      result.current.setSearchText('zzz');
    });
    act(() => {
      result.current.enterSource('claude');
    });
    expect(result.current.activeSource).toBe('claude');
    expect(result.current.browseSkillsPage).toBe(1);
    expect(result.current.searchText).toBe('');
  });
});
