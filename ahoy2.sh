#!/bin/bash
set -euo pipefail

umask 077

show_help() {
  cat <<EOF
Usage: $0 [OPTIONS]

Month-end job automation script for Ahoy system.

OPTIONS:
  -h          Show this help message
  -v          Verbose mode - print output to console as well as log file
  -f          Fast mode - skip 180 second sleep between tasks
  -s TASKS    Skip specified tasks (comma-separated list)

AVAILABLE TASKS TO SKIP:
  pricing         - Update pricing data
  login           - Login to Ahoy system
  ca_commission   - Record CA commissionable items
  us_commission   - Record US commissionable items
  ca_totals       - Group and write CA category totals
  us_totals       - Group and write US category totals
  hr_rep          - Generate HR sales rep monthly report
  hr_manager      - Generate HR sales manager monthly report

EXAMPLES:
  $0                              # Run all tasks with 180s delays
  $0 -v                           # Verbose output
  $0 -f                           # Fast mode (no delays)
  $0 -s pricing,login             # Skip pricing and login tasks
  $0 -v -f -s ca_totals,us_totals # Verbose, fast, skip totals

OUTPUT:
  Each run creates a timestamped log file in /var/log/month_end_jobs/
  Format: run_YYYYMMDD_HHMMSS.log

EOF
  exit 0
}

verbose=0
fast=0
skip_list=""

while getopts "hvfs:" opt; do
  case $opt in
    h) show_help ;;
    v) verbose=1 ;;
    f) fast=1 ;;
    s) skip_list="$OPTARG" ;;
    *) echo "Usage: $0 [-h] [-v] [-f] [-s task1,task2,...]" >&2; exit 1 ;;
  esac
done

should_skip() {
  local task="$1"
  [[ ",$skip_list," == *",$task,"* ]]
}

log_msg() {
  local msg="$1"
  echo "$msg" >>"$log_file"
  [[ $verbose -eq 1 ]] && echo "$msg"
}

do_sleep() {
  if [[ $fast -eq 0 ]]; then
    [[ $verbose -eq 1 ]] && echo "Sleeping 180s..."
    sleep 180
  fi
}

source "$(dirname "$0")/.ahoy_creds"

# Create log directory if it doesn't exist
log_dir="/var/log/month_end_jobs"
mkdir -p "$log_dir"

# Create timestamped log file for this run
timestamp="$(date +%Y%m%d_%H%M%S)"
log_file="$log_dir/run_${timestamp}.log"

cookie_file="$(mktemp /tmp/ahoy_cookies.XXXXXX)"
trap 'rm -f "$cookie_file"' EXIT

log_msg "=== $(date -Is) month_end start ==="
log_msg "Log file: $log_file"

if should_skip "pricing"; then
  log_msg "Skipping pricing"
else
  log_msg "Running pricing..."
  if [[ $verbose -eq 1 ]]; then
    curl -k -fS -L "https://shop.performanceradiator.com/indeed/_core/updatePricing.php" 2>&1 | tee -a "$log_file" || true
  else
    curl -k -fS -L "https://shop.performanceradiator.com/indeed/_core/updatePricing.php" >>"$log_file" 2>&1 || true
  fi
  echo >>"$log_file"
  do_sleep
fi

if should_skip "login"; then
  log_msg "Skipping login"
else
  log_msg "Running login..."
  if [[ $verbose -eq 1 ]]; then
    curl -k -fS -L \
      -c "$cookie_file" \
      -d "email=${ahoy_user}" \
      -d "password=${ahoy_pass}" \
      "https://ahoy.radiatorusa.com/settings/login" 2>&1 | tee -a "$log_file" || true
  else
    curl -k -fS -L \
      -c "$cookie_file" \
      -d "email=${ahoy_user}" \
      -d "password=${ahoy_pass}" \
      "https://ahoy.radiatorusa.com/settings/login" >>"$log_file" 2>&1 || true
  fi
  echo >>"$log_file"
  do_sleep
fi

run_url() {
  local task="$1"
  local url="$2"
  if should_skip "$task"; then
    log_msg "Skipping $task"
    return
  fi
  log_msg "=== $(date -Is) $task: $url ==="
  if [[ $verbose -eq 1 ]]; then
    curl -k -fS -L -b "$cookie_file" "https://ahoy.radiatorusa.com${url}" 2>&1 | tee -a "$log_file" || true
  else
    curl -k -fS -L -b "$cookie_file" "https://ahoy.radiatorusa.com${url}" >>"$log_file" 2>&1 || true
  fi
  echo >>"$log_file"
  do_sleep
}

run_url "ca_commission" "/mssql/recordCommissionableItems/ca/"
run_url "us_commission" "/mssql/recordCommissionableItems/us/"
run_url "ca_totals" "/mssql/groupAndWriteCatTotals/ca/"
run_url "us_totals" "/mssql/groupAndWriteCatTotals/us/"
run_url "hr_rep" "/sales/generateHRSalesRepMonthlyReport/$(date +%Y)/$(date +%m)"
run_url "hr_manager" "/sales/generateHRSalesManagerMonthlyReport/$(date +%Y)/$(date +%m)"

log_msg "=== $(date -Is) month_end done ==="

