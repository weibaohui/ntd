#!/bin/bash
# 批量采集各执行器的真实输出样本
# 用法: bash docs/samples/collect_samples.sh
#
# 对每个已安装的执行器，发送 "Run the commands: date and whoami, output the results."
# 将 stdout/stderr 分别保存到 docs/samples/<executor>/output.txt
#
# 注意: 运行此脚本会调用 AI 执行器，可能产生 API 费用并耗时较长。

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SAMPLES_DIR="$SCRIPT_DIR"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
TIMEOUT_SEC=180  # 每个执行器超时时间（秒）

# 统一的 prompt：让 AI 执行 date 和 whoami 并输出结果
PROMPT='Run the commands: date and whoami, output the results.'

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
  "hermes|Hermes|hermes chat -q --yolo|no"
  "kimi|Kimi|kimi --print --output-format stream-json -p|no"
  "mimo|Mimo|mimo run --format json --dangerously-skip-permissions|no"
  "zhanlu|Zhanlu|zl run --format json --dangerously-skip-permissions|no"
)

echo "=========================================="
echo "执行器输出样本采集"
echo "Prompt: $PROMPT"
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

for entry in "${EXECUTORS[@]}"; do
  IFS='|' read -r dir_name display_name cmd_str needs_stdin <<< "$entry"
  IFS=' ' read -r -a cmd_parts <<< "$cmd_str"
  
  exec_path="${cmd_parts[0]}"
  sample_dir="$SAMPLES_DIR/$dir_name"
  output_file="$sample_dir/output.txt"
  
  echo "----------------------------------------"
  echo "[$display_name] ($exec_path)"
  
  # 检查执行器是否存在
  if ! which "$exec_path" >/dev/null 2>&1; then
    echo "  ⚠️  未安装，跳过"
    continue
  fi
  
  # 构建完整命令
  full_cmd=("${cmd_parts[@]:1}" "$PROMPT")
  
  echo "  命令: $exec_path ${full_cmd[*]}"
  
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
    echo "# $display_name 真实输出样本"
    echo "# 采集时间: $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
    echo "# Prompt: $PROMPT"
    echo "# 执行命令: $exec_path ${full_cmd[*]}"
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
  f="$SAMPLES_DIR/$dir_name/output.txt"
  if [ -f "$f" ]; then
    echo "  ✅ $dir_name/output.txt ($(wc -c < "$f") bytes)"
  else
    echo "  ❌ $dir_name/output.txt (缺失)"
  fi
done
echo "=========================================="
