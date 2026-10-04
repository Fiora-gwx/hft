#!/usr/bin/env bash
# Send one message to the alert group (docs/development.md#凭证与权限): a send-only Telegram bot
# whose token and chat id are the repository secrets HFT_ALERT_TG_TOKEN / HFT_ALERT_TG_CHAT.
# Without them the message only goes to the job log and summary (warning, not failure).
set -uo pipefail

msg=${1:?message}
if [[ -z ${HFT_ALERT_TG_TOKEN:-} || -z ${HFT_ALERT_TG_CHAT:-} ]]; then
  echo "::warning::alert channel not configured (HFT_ALERT_TG_TOKEN/HFT_ALERT_TG_CHAT); message not sent"
  printf '%s\n' "$msg"
  exit 0
fi
if ! curl -sS --fail --max-time 15 -o /dev/null \
  "https://api.telegram.org/bot${HFT_ALERT_TG_TOKEN}/sendMessage" \
  --data-urlencode "chat_id=${HFT_ALERT_TG_CHAT}" \
  --data-urlencode "text=${msg}"; then
  echo "::warning::alert send failed"
fi
exit 0
