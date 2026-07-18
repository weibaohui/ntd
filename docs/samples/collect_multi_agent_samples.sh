#!/bin/bash
# 批量采集各执行器在「多 Agent 协作」场景下的真实输出
# 用法: bash docs/samples/collect_multi_agent_samples.sh
#
# 任务：让主 AI 自任「刘会计」角色，并行启动两个子 Agent：
#   - 张三丰（加法专家）计算 8+8
#   - 李雷（乘法专家）计算 10*10
# 主 AI 汇总两个子 Agent 的结果后输出总和。
# 将 stdout/stderr 分别保存到 docs/samples/<executor>/多agent测试结果.txt
#
# 注意: 运行此脚本会调用 AI 执行器，可能产生 API 费用并耗时较长。

set -euo pipefail

# 与 collect_samples.sh 保持一致的目录定位方式
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SAMPLES_DIR="$SCRIPT_DIR"
TIMEOUT_SEC=300  # 多 Agent 流程比单任务慢，超时放宽

# 任务 prompt：明确要求多 Agent 协作，便于横向比较各执行器的实现差异
read -r -d '' PROMPT <<'EOF' || true
你是计算团队负责人：刘会计。
我们需要你用多 Agent 功能进行计算。

Agent 1 名字叫张三丰，张三丰是加法计算专家，计算完成返回计算结果给刘会计：负责计算 8+8
Agent 2 名字叫李雷，李雷是乘法计算专家，计算完成返回计算结果给刘会计：负责计算 10*10

刘会计你负责用多 Agent 启动这两个 Agent，分别给他们命名，赋予他们自己的专家角色，请他们开始计算，并根据他们的结果合计输出总和。

这个计算很简单，但是我们要考查你运用多 Agent 进行并行工作的能力。我们会观察你的全部调用过程及结果。
EOF

# 定义执行器列表：(目录名, 显示名, 命令数组, 是否需要 stdin 输入)
# 数组格式: (executable arg1 arg2 ... "<prompt>")
EXECUTORS=(
  "claudecode|ClaudeCode|claude --dangerously-skip-permissions -p --output-format stream-json --verbose|no"
  "opencode|Opencode|opencode run --format json --dangerously-skip-permissions|no"
  "atomcode|Atomcode|atomcode -v --dangerously-skip-permissions -p|no"
  "pi|Pi|pi -p --mode json|yes"
  "kilo|Kilo|kilo run --format json --dangerously-skip-permissions|no"
  "codex|Codex|codex exec --json --dangerously-bypass-approvals-and-sandbox --skip-git-repo-check|no"
  "codebuddy|Codebuddy|codebuddy -p --output-format stream-json --verbose|no"
  "codewhale|Codewhale|codewhale exec --auto --output-format stream-json|no"
  "hermes|Hermes|hermes chat --yolo -q|no"
  "kimi|Kimi|kimi --output-format stream-json -p|no"
  "mimo|Mimo|mimo run --format json --dangerously-skip-permissions|no"
  "zhanlu|Zhanlu|zl run --format json --dangerously-skip-permissions|no"
)

echo "=========================================="
echo "多 Agent 协作样本采集"
echo "任务: 刘会计派生张三丰(8+8) + 李雷(10*10) 并汇总"
echo "输出目录: $SAMPLES_DIR"
echo "超时: ${TIMEOUT_SEC}s"
echo "=========================================="
echo ""

# macOS 没有 timeout 命令，用 shell 函数实现
_timeout() {
  local seconds="$1"; shift
  local pid
  # 在后台运行命令
  "$@" &
  pid=$!
  # 在后台启动超时杀手
  (sleep "$seconds" && kill "$pid" 2>/dev/null) &
  local killer_pid=$!
  wait "$pid" 2>/dev/null
  local rc=$?
  # 如果是因为被 kill 退出（rc=143=SIGTERM 或 137=SIGKILL），返回 124
  if [ $rc -eq 143 ] || [ $rc -eq 137 ]; then
    return 124
  fi
  # 清理杀手进程（如果命令提前结束）
  kill "$killer_pid" 2>/dev/null || true
  return $rc
}

# 临时目录存放单个执行器的输出
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

# 输出文件名（中文，保持 UTF-8）
OUTPUT_FILENAME="多agent测试结果.txt"

for entry in "${EXECUTORS[@]}"; do
  IFS='|' read -r dir_name display_name cmd_str needs_stdin <<< "$entry"
  IFS=' ' read -r -a cmd_parts <<< "$cmd_str"

  exec_path="${cmd_parts[0]}"
  sample_dir="$SAMPLES_DIR/$dir_name"
  output_file="$sample_dir/$OUTPUT_FILENAME"

  echo "----------------------------------------"
  echo "[$display_name] ($exec_path)"

  # 检查执行器是否存在
  if ! which "$exec_path" >/dev/null 2>&1; then
    echo "  ⚠️  未安装，跳过"
    continue
  fi

  # 构建完整命令（prompt 作为最后一个参数）
  full_cmd=("${cmd_parts[@]:1}" "$PROMPT")

  echo "  命令: $exec_path ${full_cmd[*]:0:80}..."

  # 运行并捕获输出
  stdout_file="$TMPDIR/${dir_name}_stdout.txt"
  stderr_file="$TMPDIR/${dir_name}_stderr.txt"

  set +e
  if [ "$needs_stdin" = "yes" ]; then
    # 需要 stdin 输入（如 pi 需要输入 y 确认）
    echo "y" | _timeout "$TIMEOUT_SEC" "$exec_path" "${full_cmd[@]}" > "$stdout_file" 2>"$stderr_file"
    exit_code=$?
  else
    _timeout "$TIMEOUT_SEC" "$exec_path" "${full_cmd[@]}" > "$stdout_file" 2>"$stderr_file"
    exit_code=$?
  fi
  set -e

  stdout_size=$(wc -c < "$stdout_file" 2>/dev/null || echo 0)
  stderr_size=$(wc -c < "$stderr_file" 2>/dev/null || echo 0)

  if [ $exit_code -eq 124 ]; then
    echo "  ⏰ 超时 (${TIMEOUT_SEC}s)"
  else
    echo "  退出码: $exit_code | stdout: ${stdout_size}bytes | stderr: ${stderr_size}bytes"
  fi

  # 写入输出文件
  {
    echo "# $display_name 多 Agent 协作真实输出样本"
    echo "# 采集时间: $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
    echo "# 任务: 刘会计派生张三丰(8+8) + 李雷(10*10) 并汇总"
    # 用 * 而非 @：双引号内数组切片需合并为单个字符串（ShellCheck SC2145）。
    echo "# 执行命令: $exec_path ${cmd_parts[*]:1} <prompt>"
    echo "# 退出码: $exit_code"
    echo ""
    echo "## stdout"
    echo '```'
    cat "$stdout_file" 2>/dev/null || echo "(no output)"
    echo '```'
    echo ""
    if [ -s "$stderr_file" ]; then
      echo "## stderr"
      echo '```'
      cat "$stderr_file"
      echo '```'
    fi
  } > "$output_file"

  echo "  ✅ 已保存到 $output_file"
  echo ""
done

echo "=========================================="
echo "采集完成！"
echo "样本文件位置:"
for entry in "${EXECUTORS[@]}"; do
  IFS='|' read -r dir_name display_name _ _ <<< "$entry"
  f="$SAMPLES_DIR/$dir_name/$OUTPUT_FILENAME"
  if [ -f "$f" ]; then
    echo "  ✅ $dir_name/$OUTPUT_FILENAME ($(wc -c < "$f") bytes)"
  else
    echo "  ❌ $dir_name/$OUTPUT_FILENAME (缺失)"
  fi
done
echo "=========================================="
