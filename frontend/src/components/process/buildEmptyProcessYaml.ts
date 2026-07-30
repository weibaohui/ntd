// buildEmptyProcessYaml.ts
// ---------------------------------------------------------------------------
// M6 里程碑：根据元信息构造空工艺 YAML 文本。
//
// 设计意图（对应 docs/design/029-M6-新建工艺流程-方案.md §3.1.2）：
// - 把"元信息 → 空工艺 YAML"逻辑抽成纯函数，便于单元测试（vitest）。
// - Modal 提交时调用，输出 YAML 文本 POST 到后端。
// - 复用 M5 的 yamlDump 纯函数，不手拼字符串（避免缩进/转义错误）。
//
// 边界处理：
// - 可选字段缺失时不输出该键（yamlDump 自动跳过 undefined）
// - phases 恒为空数组（空工艺）
// ---------------------------------------------------------------------------

import { yamlDump } from './processYamlValidator';

// 元信息输入，Modal 表单收集的 6 字段 + 040 的 guid
export interface ProcessMetaInput {
  // 工艺标识名（文件名/展示用，040 起不再唯一）
  name: string;
  // 040：稳定身份（UUID v4），随文件走；由调用方（Modal）用 crypto.randomUUID() 生成
  guid: string;
  // 列表页显示名（必填）
  display_name: string;
  // 工艺描述（可空）
  description?: string;
  // 类别（可空，默认 software）
  category?: string;
  // 复杂度（可空，默认 lightweight）
  complexity?: string;
  // 语义版本（可空，默认 1.0.0）
  version?: string;
}

// 根据元信息构造空工艺 YAML 文本。
//
// 输出结构：
//   process:
//     name: <name>
//     display_name: <display_name>
//     description: <description>   // 仅当提供时输出
//     category: <category>         // 仅当提供时输出
//     complexity: <complexity>     // 仅当提供时输出
//     version: <version>           // 仅当提供时输出
//   phases: []
//
// 可选字段用 undefined 让 yamlDump 自动跳过，避免输出 null。
export function buildEmptyProcessYaml(meta: ProcessMetaInput): string {
  // 构造工艺对象：process 块只含提供的字段，phases 恒为空数组
  // 040：guid 紧跟 name 之后，作为工艺的稳定身份写入文件
  const processObj: Record<string, unknown> = {
    name: meta.name,
    guid: meta.guid,
    display_name: meta.display_name,
  };
  // 可选字段仅在提供时加入（undefined 会被 yamlDump 跳过）
  if (meta.description !== undefined && meta.description !== '') {
    processObj.description = meta.description;
  }
  if (meta.category !== undefined && meta.category !== '') {
    processObj.category = meta.category;
  }
  if (meta.complexity !== undefined && meta.complexity !== '') {
    processObj.complexity = meta.complexity;
  }
  if (meta.version !== undefined && meta.version !== '') {
    processObj.version = meta.version;
  }

  // 空工艺：phases 恒为空数组
  const emptyProcess = {
    process: processObj,
    phases: [],
  };
  return yamlDump(emptyProcess);
}
