// 专家来源标签组件：系统（蓝）/ 用户（绿）。
// ExpertCard / TeamCard / ExpertDetailModal 三处曾各自复制同一段 Tag JSX，
// 抽成单点维护，避免改配色/文案时散弹式修改（Shotgun Surgery）。
// 配色与模板管理 Tab 的「系统/用户」一致：蓝=系统内置（只读），绿=用户自定义（可分享/可编辑）。

import { Tag } from 'antd';
import type { ExpertSource } from '@/types/expert';

/**
 * 专家来源标签
 *
 * @param source 专家来源（system / user）
 * @param size 展示尺寸：'card' 用于卡片头部（更紧凑），'modal' 用于详情弹窗头部
 */
export function ExpertSourceTag({
  source,
  size = 'card',
}: {
  source: ExpertSource;
  size?: 'card' | 'modal';
}) {
  // 三元一次判定，两个展示位（文字/配色）共用同一布尔，避免两处重复判断漂移
  const isSystem = source === 'system';
  return (
    <Tag
      color={isSystem ? 'blue' : 'green'}
      style={
        size === 'modal'
          ? { fontSize: 11, margin: 0 }
          : { margin: 0, fontSize: 10, padding: '1px 6px' }
      }
    >
      {isSystem ? '系统' : '用户'}
    </Tag>
  );
}
