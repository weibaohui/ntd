/**
 * useSkillInstall — 技能安装 Modal 数据族 hook（096-W4-4 产物）。
 *
 * 承接原 SkillMarketplace 的安装区块：installModalOpen/targetExecutors/installing
 * 3 个 state + handleOpenInstall/handleInstall 编排（逐执行器循环安装并汇总结果）。
 * 函数体逐字搬自主组件原实现，行为等价。
 */

import { useState } from 'react';
import { App } from 'antd';
import { bundledApi, type BundledSkillMeta } from '@/api/bundled';

export interface SkillInstallState {
  installModalOpen: boolean;
  targetExecutors: string[];
  installing: boolean;
  setTargetExecutors: (v: string[]) => void;
  /** 打开安装弹窗（清空上次的目标选择） */
  openInstall: () => void;
  /** 关闭安装弹窗 */
  closeInstall: () => void;
  /** 逐执行器安装并汇总提示；返回是否有发起安装（无选中技能/无目标时为 false） */
  install: (selectedSkill: BundledSkillMeta | null, onDone: () => void) => Promise<void>;
}

export function useSkillInstall(): SkillInstallState {
  const { message } = App.useApp();
  const [installModalOpen, setInstallModalOpen] = useState(false);
  const [targetExecutors, setTargetExecutors] = useState<string[]>([]);
  const [installing, setInstalling] = useState(false);

  const openInstall = () => {
    setTargetExecutors([]);
    setInstallModalOpen(true);
  };

  const closeInstall = () => setInstallModalOpen(false);

  const install = async (selectedSkill: BundledSkillMeta | null, onDone: () => void) => {
    if (!selectedSkill || targetExecutors.length === 0) return;
    setInstalling(true);
    const shortName = selectedSkill.short_name;
    const results: string[] = [];
    for (const executor of targetExecutors) {
      try {
        await bundledApi.installSkill(selectedSkill.name, executor);
        results.push(`${executor}: 成功`);
      } catch (e: any) {
        results.push(`${executor}: 失败 (${e?.message || e})`);
      }
    }
    setInstalling(false);
    setInstallModalOpen(false);
    const successCount = results.filter(r => r.includes('成功')).length;
    if (successCount === targetExecutors.length) {
      message.success(`${shortName} 已安装到 ${successCount} 个执行器`);
    } else {
      message.warning(`安装完成: ${successCount}/${targetExecutors.length} 成功`);
    }
    // 安装完成后由调用方刷新已安装列表（原实现即 loadInstalled()）
    onDone();
  };

  return {
    installModalOpen,
    targetExecutors,
    installing,
    setTargetExecutors,
    openInstall,
    closeInstall,
    install,
  };
}
