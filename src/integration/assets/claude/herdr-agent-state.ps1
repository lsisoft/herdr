# installed by herdr
# managed by herdr; reinstalling or updating the integration overwrites this file.
# add custom hooks beside this file instead of editing it.
# HERDR_INTEGRATION_ID=claude
# HERDR_INTEGRATION_VERSION=8

param([string]$Action = "")

if ($Action -ne "session") { exit 0 }
if ($env:HERDR_ENV -ne "1") { exit 0 }
if ([string]::IsNullOrWhiteSpace($env:HERDR_PANE_ID)) { exit 0 }

$inputText = [Console]::In.ReadToEnd()
try {
    $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { $null } else { $inputText | ConvertFrom-Json }
} catch {
    exit 0
}

if (-not [string]::IsNullOrWhiteSpace($payload.agent_id)) { exit 0 }
if ($payload.hook_event_name -eq "SubagentStop") { exit 0 }

$sessionId = $payload.session_id
if ([string]::IsNullOrWhiteSpace($sessionId)) { exit 0 }

$runtime = [ordered]@{}
$model = if ($payload.hook_event_name -eq "SessionStart" -and $payload.model -is [string]) { $payload.model } else { $null }
if ([string]::IsNullOrWhiteSpace($model) -and $payload.transcript_path -is [string] -and (Test-Path -LiteralPath $payload.transcript_path)) {
    try {
        Get-Content -LiteralPath $payload.transcript_path -Tail 512 | ForEach-Object {
            if ($_ -match '"type"\s*:\s*"assistant"') {
                try {
                    $item = $_ | ConvertFrom-Json
                    if ($item.message.model -is [string] -and -not [string]::IsNullOrWhiteSpace($item.message.model)) {
                        $model = $item.message.model
                    }
                } catch {
                }
            }
        }
    } catch {
    }
}
if (-not [string]::IsNullOrWhiteSpace($model)) {
    $runtime.model = $model
}
if ($payload.effort.level -is [string] -and -not [string]::IsNullOrWhiteSpace($payload.effort.level)) {
    $runtime.reasoning_effort = $payload.effort.level
}

$seq = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
try {
    $args = @(
        "pane",
        "report-agent-session",
        $env:HERDR_PANE_ID,
        "--source",
        "herdr:claude",
        "--agent",
        "claude",
        "--seq",
        "$seq",
        "--agent-session-id",
        "$sessionId"
    )
    if ($payload.transcript_path -is [string] -and -not [string]::IsNullOrWhiteSpace($payload.transcript_path)) {
        $args += @("--agent-session-path", "$($payload.transcript_path)")
    }
    if ($payload.hook_event_name -eq "SessionStart" -and $payload.source -is [string] -and -not [string]::IsNullOrWhiteSpace($payload.source)) {
        $args += @("--session-start-source", "$($payload.source)")
    }
    if ($runtime.Count -gt 0) {
        $args += @("--runtime-json", ($runtime | ConvertTo-Json -Compress))
    }
    & herdr @args 2>$null | Out-Null
} catch {
}
