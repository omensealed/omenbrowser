#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 BASELINE_RESULTS CURRENT_RESULTS" >&2
  exit 2
fi

baseline="$1"
current="$2"
for result in "$baseline" "$current"; do
  for file in metadata.txt summary.txt; do
    if [[ ! -f "$result/$file" ]]; then
      echo "missing idle measurement file: $result/$file" >&2
      exit 2
    fi
  done
done

value() {
  local file="$1" key="$2"
  awk -F= -v key="$key" '$1 == key {sub(/^[^=]*=/, ""); print; exit}' "$file"
}

perf_task_clock() {
  local result="$1" measured
  measured="$(value "$result/summary.txt" perf_task_clock_ms)"
  if [[ -n "$measured" ]]; then
    printf '%s\n' "$measured"
  elif [[ -f "$result/perf-stat.txt" ]]; then
    awk '$2 == "msec" && $3 ~ /^task-clock/ {print $1; exit}' "$result/perf-stat.txt"
  fi
}

baseline_warmup="$(value "$baseline/metadata.txt" warmup_seconds)"
current_warmup="$(value "$current/metadata.txt" warmup_seconds)"
baseline_sample="$(value "$baseline/metadata.txt" sample_seconds)"
current_sample="$(value "$current/metadata.txt" sample_seconds)"
if [[ "$baseline_warmup:$baseline_sample" != "$current_warmup:$current_sample" ]]; then
  echo "idle measurements must use identical warmup/sample durations" >&2
  exit 1
fi

baseline_cpu="$(value "$baseline/summary.txt" cpu_percent_median)"
current_cpu="$(value "$current/summary.txt" cpu_percent_median)"
baseline_cpu_p95="$(value "$baseline/summary.txt" cpu_percent_p95)"
current_cpu_p95="$(value "$current/summary.txt" cpu_percent_p95)"
baseline_switches="$(value "$baseline/summary.txt" scheduler_context_switch_proxy_per_minute)"
current_switches="$(value "$current/summary.txt" scheduler_context_switch_proxy_per_minute)"
baseline_rss="$(value "$baseline/summary.txt" rss_kib_median)"
current_rss="$(value "$current/summary.txt" rss_kib_median)"
baseline_dirty="$(value "$baseline/summary.txt" private_dirty_kib_median)"
current_dirty="$(value "$current/summary.txt" private_dirty_kib_median)"
baseline_messages="$(value "$baseline/metadata.txt" recurring_app_messages_per_minute)"
current_messages="$(value "$current/metadata.txt" recurring_app_messages_per_minute)"
baseline_task_clock="$(perf_task_clock "$baseline")"
current_task_clock="$(perf_task_clock "$current")"

awk \
  -v warmup="$baseline_warmup" -v sample="$baseline_sample" \
  -v bcpu="$baseline_cpu" -v ccpu="$current_cpu" \
  -v bcpu95="$baseline_cpu_p95" -v ccpu95="$current_cpu_p95" \
  -v bsw="$baseline_switches" -v csw="$current_switches" \
  -v brss="$baseline_rss" -v crss="$current_rss" \
  -v bdirty="$baseline_dirty" -v cdirty="$current_dirty" \
  -v bmsg="$baseline_messages" -v cmsg="$current_messages" \
  -v bclock="$baseline_task_clock" -v cclock="$current_task_clock" '
  function reduction(before, after) {return before > 0 ? 100 * (before-after) / before : 0}
  function delta(before, after) {return before > 0 ? 100 * (after-before) / before : 0}
  BEGIN {
    printf "warmup_seconds=%s\nsample_seconds=%s\n", warmup, sample
    printf "baseline_cpu_percent_median=%.3f\ncurrent_cpu_percent_median=%.3f\ncpu_median_reduction_percent=%.2f\n", bcpu, ccpu, reduction(bcpu,ccpu)
    printf "baseline_cpu_percent_p95=%.3f\ncurrent_cpu_percent_p95=%.3f\ncpu_p95_reduction_percent=%.2f\n", bcpu95, ccpu95, reduction(bcpu95,ccpu95)
    printf "baseline_scheduler_context_switch_proxy_per_minute=%.3f\ncurrent_scheduler_context_switch_proxy_per_minute=%.3f\ncontext_switch_proxy_reduction_percent=%.2f\n", bsw, csw, reduction(bsw,csw)
    printf "baseline_rss_kib_median=%.0f\ncurrent_rss_kib_median=%.0f\nrss_delta_percent=%.2f\n", brss, crss, delta(brss,crss)
    printf "baseline_private_dirty_kib_median=%.0f\ncurrent_private_dirty_kib_median=%.0f\nprivate_dirty_delta_percent=%.2f\n", bdirty, cdirty, delta(bdirty,cdirty)
    if (bclock != "" && cclock != "") {
      printf "baseline_perf_task_clock_ms=%.2f\ncurrent_perf_task_clock_ms=%.2f\nperf_task_clock_reduction_percent=%.2f\n", bclock, cclock, reduction(bclock,cclock)
    } else {
      print "perf_task_clock_comparison=pending"
    }
    if (bmsg == "pending" || cmsg == "pending") {
      print "recurring_application_messages=pending"
      print "application_message_reduction_gate=pending"
    } else {
      printf "baseline_recurring_app_messages_per_minute=%d\ncurrent_recurring_app_messages_per_minute=%d\napplication_message_reduction_percent=%.2f\n", bmsg, cmsg, reduction(bmsg,cmsg)
      print reduction(bmsg,cmsg) >= 80 ? "application_message_reduction_gate=pass" : "application_message_reduction_gate=fail"
    }
    print (ccpu <= 1 || reduction(bcpu,ccpu) >= 65) ? "idle_cpu_gate=pass" : "idle_cpu_gate=fail"
    print "context_switch_proxy_is_not_application_message_count=yes"
  }
'
