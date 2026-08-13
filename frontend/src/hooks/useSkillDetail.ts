/**
 * useSkillDetail — 技能详情 Drawer 数据族 hook（096-W4-4 产物）。
 *
 * 承接原 SkillMarketplace 的详情区块：selectedSkill/drawerOpen/content/files/
 * contentLoading 5 个 state + detailReqIdRef 竞态守卫 + handleCardClick 编排。
 * 函数体逐字搬自主组件原实现，行为等价。
 */

import { useRef, useState } from 'react';
import { bundledApi, type BundledSkillFile, type BundledSkillMeta } from '@/api/bundled';

export interface SkillDetailState {
  selectedSkill: BundledSkillMeta | null;
  drawerOpen: boolean;
  content: string;
  files: BundledSkillFile[];
  contentLoading: boolean;
  /** 点击技能卡片：打开 Drawer 并异步加载内容（竞态守卫——晚到的旧请求结果被丢弃） */
  openDetail: (skill: BundledSkillMeta) => Promise<void>;
  /** 关闭 Drawer */
  closeDetail: () => void;
}

export function useSkillDetail(): SkillDetailState {
  const [selectedSkill, setSelectedSkill] = useState<BundledSkillMeta | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [content, setContent] = useState('');
  const [files, setFiles] = useState<BundledSkillFile[]>([]);
  const [contentLoading, setContentLoading] = useState(false);

  // 详情请求竞态守卫：每次点击自增并记下本次序号；晚返回的旧请求若发现序号已变就丢弃结果，
  // 避免快速连点 A→B 时 A 的内容把 B 的详情覆盖掉（旧请求的 finally 也不会误关 B 的 loading）。
  const detailReqIdRef = useRef(0);

  const openDetail = async (skill: BundledSkillMeta) => {
    // 先占坑：立即清空旧内容并打开 Drawer，让用户感到响应即时；
    // 真正的内容等异步返回、且确认仍是最新请求后才写入。
    const reqId = ++detailReqIdRef.current;
    setSelectedSkill(skill);
    setDrawerOpen(true);
    setContent('');
    setFiles([]);
    setContentLoading(true);
    try {
      const res = await bundledApi.getSkillContent(skill.name);
      // 序号已变 → 等待期间用户又点了别的技能，丢弃这次过期结果，不覆盖新选中技能。
      if (reqId !== detailReqIdRef.current) return;
      setContent(res.content);
      setFiles(res.files);
    } catch {
      if (reqId !== detailReqIdRef.current) return;
      setContent('加载内容失败');
    } finally {
      // 只关「最新那次」请求的 loading；过期请求的 loading 由接管它的新请求自己管理。
      if (reqId === detailReqIdRef.current) setContentLoading(false);
    }
  };

  const closeDetail = () => setDrawerOpen(false);

  return {
    selectedSkill,
    drawerOpen,
    content,
    files,
    contentLoading,
    openDetail,
    closeDetail,
  };
}
