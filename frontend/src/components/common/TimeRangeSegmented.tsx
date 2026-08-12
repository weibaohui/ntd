// 全站共享的时间过滤分段组件。
// 为什么存在：看板页（原 KanbanBoard）与 原运行中心 曾各持有一份相同的 TIME_OPTIONS
// 和 label↔value 映射渲染逻辑（两处完全重复），任务页接入时间过滤时会出现第三份。
// 收敛为单一组件后，后期新增时间选项只需改 TIME_RANGE_OPTIONS 一处，各页面同步生效。
//
// 设计要点：
// 1. 「全部」不是时间选项，而是「不过滤」状态，因此用 number | null 表达值域（null = 全部）。
// 2. Props 用判别联合：showAll 为字面量判别字段。不带 showAll 的调用方在类型层面
//    就不可能收到 null，无需运行时判空（看板页场景）；带 showAll 的调用方（任务页）
//    则必须处理 null。

import { Segmented } from 'antd';
import type { CSSProperties } from 'react';

/**
 * 时间选项：label 展示文本，value 小时数。
 * 全站唯一事实源 —— 后期新增时间选项只改这里，所有使用页面同步更新。
 * 数值口径沿用历史实现：3d=72h、7d=168h。
 */
export const TIME_RANGE_OPTIONS: { label: string; value: number }[] = [
  { label: '6h', value: 6 },
  { label: '12h', value: 12 },
  { label: '24h', value: 24 },
  { label: '3d', value: 72 },
  { label: '7d', value: 168 },
];

// AntD Segmented 的 value 是字符串值域，需要哨兵串表达「全部」。
// 选用不可能与任何时间 label 冲突的取值，避免未来新增选项时撞名。
const ALL_VALUE = '__all__';

/** 「全部」选项的展示文本，抽成常量便于测试断言与将来统一调整文案。 */
const ALL_LABEL = '全部';

/** 两种形态共用的基础属性。 */
interface TimeRangeBaseProps {
  // 默认 small：被替换的两处历史实现均用 size="small"，保持视觉一致。
  size?: 'small' | 'middle' | 'large';
  // 看板页原实现带 marginLeft: 8 的间距样式，透传 style 以原样保留布局。
  style?: CSSProperties;
}

/** showAll 形态（任务页）：value 允许 null（全部），onChange 可能回传 null。 */
interface TimeRangeWithAllProps extends TimeRangeBaseProps {
  // 字面量 true 构成判别字段，TS 据此收窄 value/onChange 签名。
  showAll: true;
  value: number | null;
  onChange: (hours: number | null) => void;
}

/** 非 showAll 形态（看板页）：value 必为小时数，类型保证调用方收不到 null。 */
interface TimeRangeWithoutAllProps extends TimeRangeBaseProps {
  showAll?: false;
  value: number;
  onChange: (hours: number) => void;
}

export type TimeRangeSegmentedProps = TimeRangeWithAllProps | TimeRangeWithoutAllProps;

/** 把外部数值值域映射为 Segmented 字符串值域；未匹配时按形态回退。 */
function toSegmentedValue(props: TimeRangeSegmentedProps): string {
  // null 只在 showAll 形态下合法，直接映射到哨兵串。
  if (props.value == null) return ALL_VALUE;
  const hit = TIME_RANGE_OPTIONS.find((o) => o.value === props.value);
  if (hit) return hit.label;
  // 未匹配回退与历史实现 `|| '24h'` 对齐：非 showAll 形态回退 24h；
  // showAll 形态回退「全部」（不过滤比静默选错窗口更安全）。
  return props.showAll ? ALL_VALUE : '24h';
}

/**
 * 时间过滤分段组件。
 *
 * 整体处理思路：
 * 1. 由判别联合 Props 决定是否渲染「全部」首项；
 * 2. toSegmentedValue 把 number|null 映射为 Segmented 字符串值；
 * 3. onChange 时把字符串值逆映射回 number|null 回传给调用方。
 */
export function TimeRangeSegmented(props: TimeRangeSegmentedProps) {
  const { size = 'small', style } = props;

  // 选项集：showAll 形态在首位插入「全部」，其余项与时间选项一一对应。
  // 每次 render 重建数组的开销可忽略（5~6 项），不引入 useMemo。
  const options = props.showAll
    ? [{ label: ALL_LABEL, value: ALL_VALUE }, ...TIME_RANGE_OPTIONS.map((o) => ({ label: o.label, value: o.label }))]
    : TIME_RANGE_OPTIONS.map((o) => ({ label: o.label, value: o.label }));

  // 逆映射：哨兵串 → null（仅 showAll 形态可达）；label → 对应小时数。
  // 找不到对应项时不回传（理论不可达，选项集与本映射同源），避免向上层发出非法值。
  const handleChange = (v: string | number) => {
    if (props.showAll && v === ALL_VALUE) {
      props.onChange(null);
      return;
    }
    const opt = TIME_RANGE_OPTIONS.find((o) => o.label === v);
    if (!opt) return;
    // number 对两种 onChange 签名均合法（number | null 兼容 number），无需按形态分发。
    props.onChange(opt.value);
  };

  return (
    <Segmented
      size={size}
      options={options}
      value={toSegmentedValue(props)}
      onChange={handleChange}
      style={style}
    />
  );
}
