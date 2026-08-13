/**
 * UsageStatsCard — 「AI 使用统计」卡片（096-W4-4-3 产物）。
 *
 * 从 ExecutorsPanel 拆出的全局配置族之二：自动收集本机执行器 Token 用量的定时任务开关 + cron。
 * 自治：挂载时拉取设置（getUsageStatsSettings），Switch 即时保存（updateUsageStatsSettings），
 * cron 编辑后点「保存」回写。
 */

import { useEffect, useState } from 'react';
import { App, Button, Card, Switch, Typography } from 'antd';
import { ClockCircleOutlined } from '@ant-design/icons';
import { Cron } from 'react-js-cron';
import 'react-js-cron/dist/styles.css';
import { CronPresetSelect } from '@/components/CronPresetSelect';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '@/utils/cron';
import * as db from '@/utils/database';

export function UsageStatsCard() {
  const { message } = App.useApp();
  const [usageStatsEnabled, setUsageStatsEnabled] = useState(false);
  // cron 默认每天 01:00（6 字段格式，react-js-cron 用 5 字段，经 cronTo5/cronTo6 互转）。
  const [usageStatsCron, setUsageStatsCron] = useState('0 0 1 * * *');
  const [usageStatsLoading, setUsageStatsLoading] = useState(false);
  const [usageStatsSaving, setUsageStatsSaving] = useState(false);

  /** 拉取 usage stats 设置（开关 + cron）。 */
  const loadUsageStatsSettings = async () => {
    try {
      setUsageStatsLoading(true);
      const settings = await db.getUsageStatsSettings();
      setUsageStatsEnabled(settings.auto_usage_stats_enabled);
      setUsageStatsCron(settings.auto_usage_stats_cron);
    } catch (err: unknown) {
      // 加载失败用默认值（保留 state 初值）；记录原因便于排查（禁止清单 #6：空 catch 需留痕）。
      console.warn('加载 AI 使用统计设置失败，使用默认值', err);
    } finally {
      setUsageStatsLoading(false);
    }
  };

  /** 保存 cron 编辑结果（开关已在 Switch 即时保存，此处仅回写 cron）。 */
  const handleSaveUsageStats = async () => {
    try {
      setUsageStatsSaving(true);
      await db.updateUsageStatsSettings(usageStatsEnabled, usageStatsCron);
      message.success('AI 使用统计配置已更新');
    } catch (err: unknown) {
      message.error('保存失败: ' + (err instanceof Error ? err.message : String(err)));
    } finally {
      setUsageStatsSaving(false);
    }
  };

  /**
   * Switch 即时保存：切换开关立刻回写后端（不等「保存」按钮），与原 extra Switch 行为一致。
   * 用当前 usageStatsCron 一并提交（cron 编辑未保存时提交的是上次保存值，符合原语义）。
   */
  const handleUsageStatsToggle = async (checked: boolean) => {
    setUsageStatsEnabled(checked);
    try {
      setUsageStatsSaving(true);
      await db.updateUsageStatsSettings(checked, usageStatsCron);
      message.success('AI 使用统计配置已更新');
    } catch (err: unknown) {
      message.error('保存失败: ' + (err instanceof Error ? err.message : String(err)));
    } finally {
      setUsageStatsSaving(false);
    }
  };

  // 挂载时加载设置（原主组件 useEffect 的一部分）。
  useEffect(() => {
    loadUsageStatsSettings();
  }, []);

  return (
    <Card
      size="small"
      title={<><ClockCircleOutlined style={{ marginRight: 6 }} />AI 使用统计</>}
      style={{ marginTop: 16 }}
      extra={
        <Switch
          checked={usageStatsEnabled}
          onChange={handleUsageStatsToggle}
          loading={usageStatsLoading}
        />
      }
    >
      {usageStatsEnabled && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <Typography.Paragraph type="secondary" style={{ marginBottom: 8 }}>
            自动收集本机执行器的 Token 使用量，每日归档到数据库
          </Typography.Paragraph>
          <CronPresetSelect value={usageStatsCron} onChange={(val: string) => setUsageStatsCron(val)} />
          <Cron
            value={cronTo5(usageStatsCron)}
            setValue={(val: string) => { setUsageStatsCron(cronTo6(val)); }}
            locale={CRON_ZH_LOCALE}
            defaultPeriod="day"
            humanizeLabels
            allowClear={false}
          />
          <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
            <Button size="small" type="primary" onClick={handleSaveUsageStats} loading={usageStatsSaving}>
              保存
            </Button>
          </div>
        </div>
      )}
    </Card>
  );
}
