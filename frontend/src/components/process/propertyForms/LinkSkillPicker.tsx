// LinkSkillPicker.tsx
// ---------------------------------------------------------------------------
// 环节属性面板的技能选择器（需求 053）。
//
// 设计要点（与执行器解耦）：
// - link.skills 存的是纯技能名，skill 可安装到任意执行器，因此 skills 不绑定执行器；
// - 执行器仅作为「筛选条件」内置于本选择器：切换执行器 → Table 展示该执行器的可选 skills，
//   已选 skills 跨执行器保留、不丢失；
// - 与环节属性面板上的「执行器」字段（环节运行执行器）相互独立，本组件不再读取 link.executor。
//
// 支持点选（Table 多选）+ 手填（搜索框回车添加列表外名称）。已选 Tag 区用「全量技能名集合」
// 判断是否自定义（不在任何执行器列表里 = 手填），避免把来自其他执行器的已选误判为自定义。
// ---------------------------------------------------------------------------
import { useEffect, useMemo, useState, useDeferredValue, type JSX } from 'react';
import { Form, Tag, Input, Table, Spin, Empty, Typography, type TableProps } from 'antd';
import { SearchOutlined } from '@ant-design/icons';
import { getSkillsList } from '@/utils/database/skills';
import { ExecutorPickerPopover } from '@/components/common/ExecutorPickerPopover';
import { DEFAULT_EXECUTOR } from '@/types';
import type { ExecutorSkills, SkillMeta } from '@/types';
import {
  splitSelected,
  syncFromTable,
  canAddCustom,
  addCustom,
  removeSkill,
  filterSkills,
  skillTagMeta,
} from './skillSelectionUtils';

export interface LinkSkillPickerProps {
  /** 已选技能名（= link.skills，跨执行器、与执行器无绑定） */
  selectedSkills?: string[];
  /** 整体写回新的已选数组 */
  onChange: (skills: string[]) => void;
}

// 表格列定义：技能名+描述合并为一列（窄面板省横向空间），版本单独一列。
// 抽到组件外作为常量，保持主组件体精简（纯渲染映射，无状态依赖）。
const SKILL_COLUMNS: TableProps<SkillMeta>['columns'] = [
  {
    title: '技能',
    dataIndex: 'name',
    render: (_, r: SkillMeta) => (
      <div>
        <div style={{ fontWeight: 600 }}>{r.name}</div>
        {r.description && (
          // 描述单行省略：信息密度高又不撑高行
          <div style={{ fontSize: 12, color: 'var(--color-text-tertiary)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
            {r.description}
          </div>
        )}
      </div>
    ),
  },
  {
    title: '版本',
    dataIndex: 'version',
    width: 72,
    render: (v: string | null) =>
      v ? (
        <Tag color="blue" style={{ margin: 0 }}>v{v}</Tag>
      ) : (
        <Typography.Text type="secondary">-</Typography.Text>
      ),
  },
];

/**
 * 一次性加载全部执行器的 skills（全量）。
 * 全量用于：① 按内置筛选执行器过滤出 Table 数据源；② 汇总「全量技能名集合」区分自定义手填项。
 */
function useAllSkills(): { list: ExecutorSkills[]; loading: boolean } {
  const [list, setList] = useState<ExecutorSkills[]>([]);
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    getSkillsList()
      .then((l) => {
        if (!cancelled) setList(l);
      })
      .catch(() => {
        if (!cancelled) setList([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);
  return { list, loading };
}

/** 已选 Tag 区：只区分「手填自定义」与「已知 skill」，标注稳定统一（不随筛选执行器变化）。 */
function SelectedSkillTags({
  selected,
  skillSource,
  onRemove,
}: {
  selected: string[] | undefined;
  skillSource: ReadonlyMap<string, string[]>;
  onRemove: (name: string) => void;
}): JSX.Element | null {
  if (!selected?.length) return null;
  return (
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, marginBottom: 8 }}>
      {selected.map((name) => {
        const meta = skillTagMeta(name, skillSource);
        return (
          <Tag
            key={name}
            color={meta.color}
            closable
            onClose={() => onRemove(name)}
            style={{ margin: 0 }}
          >
            {name}
            {meta.suffix && <span style={{ fontSize: 10, opacity: 0.85 }}>{meta.suffix}</span>}
          </Tag>
        );
      })}
    </div>
  );
}

export function LinkSkillPicker({
  selectedSkills,
  onChange,
}: LinkSkillPickerProps): JSX.Element {
  const { list, loading } = useAllSkills();
  // 内置执行器筛选：仅决定 Table 展示哪个执行器的可选 skills，不影响已选（已选跨执行器保留）。
  // 默认系统默认执行器；用户切换只改 Table 数据源，不写回 link。
  const [filterExecutor, setFilterExecutor] = useState<string>(DEFAULT_EXECUTOR);
  const [search, setSearch] = useState('');
  const deferred = useDeferredValue(search);

  // skill 名 → 出现它的执行器列表：供已选 tag 标注来源，区分「来自其他执行器」与「手填自定义」
  const skillSource = useMemo(() => {
    const map = new Map<string, string[]>();
    for (const e of list) {
      for (const sk of e.skills) {
        const arr = map.get(sk.name) ?? [];
        arr.push(e.executor);
        map.set(sk.name, arr);
      }
    }
    return map;
  }, [list]);

  // 当前筛选执行器的 skills + 名集合（Table 数据源 + 勾选桥接依据）。
  // 后端可能返回重复 skill 名（如 claudecode 有两个 code-refactoring），按 name 去重，
  // 避免 antd Table 重复 rowKey 导致勾选状态串、重复行渲染与已选重复。
  const currentSkills = useMemo(() => {
    const skills = list.find((e) => e.executor === filterExecutor)?.skills ?? [];
    const seen = new Set<string>();
    return skills.filter((s) => {
      if (seen.has(s.name)) return false;
      seen.add(s.name);
      return true;
    });
  }, [list, filterExecutor]);
  const currentListNames = useMemo(
    () => new Set(currentSkills.map((s) => s.name)),
    [currentSkills],
  );

  // 过滤后的表格数据 + 表格内已选 + 是否可手填
  const filtered = useMemo(() => filterSkills(currentSkills, deferred), [currentSkills, deferred]);
  const inList = useMemo(
    () => splitSelected(selectedSkills, currentListNames).inList,
    [selectedSkills, currentListNames],
  );
  const trimmed = search.trim();
  const canAdd = canAddCustom(selectedSkills, currentListNames, search);

  // 回车 / 点提示：加入自定义技能并清空搜索框
  const addNow = (): void => {
    if (!canAdd) return;
    onChange(addCustom(selectedSkills, search));
    setSearch('');
  };

  return (
    <Form.Item
      label="技能"
      tooltip="技能与执行器解耦：已选技能跨执行器保留。下方执行器仅用于筛选可选列表，可手填列表外名称"
    >
      {/* 已选汇总：显式列出当前选中，跨执行器都显示 */}
      <SelectedSkillTags
        selected={selectedSkills}
        skillSource={skillSource}
        onRemove={(n) => onChange(removeSkill(selectedSkills, n))}
      />
      {/* 执行器筛选 + 搜索框并排：执行器仅筛 Table，不绑定已选 */}
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 8 }}>
        <ExecutorPickerPopover value={filterExecutor} onChange={setFilterExecutor} />
        <Input
          prefix={<SearchOutlined style={{ color: 'var(--color-text-quaternary)' }} />}
          placeholder="搜索技能，或输入后回车添加自定义"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          onPressEnter={addNow}
          allowClear
          style={{ flex: 1 }}
        />
      </div>
      {canAdd && (
        <div
          onClick={addNow}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === 'Enter') addNow();
          }}
          style={{ fontSize: 12, color: 'var(--color-primary)', cursor: 'pointer', marginTop: 4 }}
        >
          未找到“{trimmed}”，按回车添加为自定义技能
        </div>
      )}
      {loading ? (
        <div style={{ textAlign: 'center', padding: 16 }}>
          <Spin size="small" />
        </div>
      ) : (
        <Table<SkillMeta>
          dataSource={filtered}
          columns={SKILL_COLUMNS}
          rowKey="name"
          size="small"
          pagination={filtered.length > 8 ? { pageSize: 8, size: 'small' } : false}
          locale={{
            emptyText: (
              <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="该执行器暂无 Skills，可切换执行器或在上方手填" />
            ),
          }}
          rowSelection={{
            // 只把「当前筛选执行器内已选」交给表格；其余执行器的已选手填项由 Tag 区管理，不被覆盖
            selectedRowKeys: inList,
            onChange: (keys) => onChange(syncFromTable(selectedSkills, currentListNames, keys as string[])),
          }}
        />
      )}
    </Form.Item>
  );
}
