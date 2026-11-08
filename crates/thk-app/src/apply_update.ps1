param([Parameter(Mandatory = $true)][string]$ConfigPath)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$config = Get-Content -LiteralPath $ConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
$log = "$ConfigPath.log"
$replaced = $false
$staged = $false
$lock = $null

function Write-UpdateLog([string]$Message) {
    try {
        Add-Content -LiteralPath $log -Encoding UTF8 -Value "$(Get-Date -Format o) $Message"
    } catch {
        [Console]::Error.WriteLine("Could not write update log: $($_.Exception.Message)")
    }
}

try {
    Write-UpdateLog 'Waiting for launcher exit.'
    $parent = Get-Process -Id $config.parent_id -ErrorAction Ignore
    if ($null -ne $parent) {
        try {
            if (-not $parent.WaitForExit(20000)) {
                throw 'Launcher did not exit; update was not applied.'
            }
        } finally {
            $parent.Dispose()
        }
    }

    $lock = [System.IO.File]::Open($config.lock, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
    if ((Test-Path -LiteralPath $config.staged) -or (Test-Path -LiteralPath $config.backup)) {
        throw 'Update staging or backup path already exists; refusing to overwrite recovery data.'
    }
    [System.IO.File]::Copy($config.source, $config.staged, $false)
    $staged = $true
    $actual = (Get-FileHash -LiteralPath $config.staged -Algorithm SHA256).Hash
    if ($actual -ne $config.sha256) {
        throw 'Staged update SHA-256 does not match the verified download.'
    }

    [System.IO.File]::Replace($config.staged, $config.destination, $config.backup)
    $staged = $false
    $replaced = $true
    $process = Start-Process -FilePath $config.destination -WorkingDirectory ([System.IO.Path]::GetDirectoryName($config.destination)) -PassThru
    $replaced = $false
    Write-UpdateLog "Update applied; started process $($process.Id). Backup: $($config.backup)"
    $process.Dispose()
    exit 0
} catch {
    $failure = $_.Exception.Message
    if ($replaced) {
        try {
            [System.IO.File]::Replace($config.backup, $config.destination, $config.failed)
            Write-UpdateLog 'Original executable restored.'
        } catch {
            $failure += " Rollback failed: $($_.Exception.Message). Recovery backup: $($config.backup)"
        }
    }
    Write-UpdateLog "ERROR: $failure"
    [Console]::Error.WriteLine($failure)
    exit 1
} finally {
    if ($staged) {
        try {
            [System.IO.File]::Delete($config.staged)
        } catch {
            Write-UpdateLog "Could not remove staging file: $($_.Exception.Message)"
        }
    }
    if ($null -ne $lock) {
        $lock.Dispose()
        try {
            [System.IO.File]::Delete($config.lock)
        } catch {
            Write-UpdateLog "Could not release update lock: $($_.Exception.Message)"
        }
    }
}
