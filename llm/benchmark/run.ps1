# ============================================================
# Local LLM Lightweight Benchmark
# ============================================================

$ErrorActionPreference = "Stop"

# ------------------------------------------------------------
# Paths
# ------------------------------------------------------------

$BenchmarkRoot = Split-Path -Parent $PSScriptRoot
$ConfigPath    = Join-Path $BenchmarkRoot "benchmark\config\benchmark.json"
$ResultsPath   = Join-Path $BenchmarkRoot "results"

if (-not (Test-Path $ConfigPath)) {
    Write-Host ""
    Write-Host "ERROR: Configuration file not found:"
    Write-Host $ConfigPath
    exit 1
}

New-Item -ItemType Directory -Force -Path $ResultsPath | Out-Null

try {
    $config = Get-Content $ConfigPath -Raw | ConvertFrom-Json
}
catch {
    Write-Host ""
    Write-Host "ERROR: Could not parse configuration file:"
    Write-Host $ConfigPath
    Write-Host ""
    Write-Host $_
    exit 1
}

# ------------------------------------------------------------
# Validate configuration
# ------------------------------------------------------------

if (-not $config.llama_cpp) {
    Write-Host ""
    Write-Host "ERROR: 'llama_cpp' is not defined in benchmark.json"
    exit 1
}

if (-not $config.models) {
    Write-Host ""
    Write-Host "ERROR: No models are defined in benchmark.json"
    exit 1
}

if (-not $config.experiments) {
    Write-Host ""
    Write-Host "ERROR: No experiments are defined in benchmark.json"
    exit 1
}

$LlamaCpp   = $config.llama_cpp
$LlamaBench = Join-Path $LlamaCpp "build\bin\llama-bench.exe"

if (-not (Test-Path $LlamaBench)) {
    Write-Host ""
    Write-Host "ERROR: llama-bench.exe not found:"
    Write-Host $LlamaBench
    exit 1
}

# ------------------------------------------------------------
# Timestamp / Result directory
# ------------------------------------------------------------

$Timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$ResultDir = Join-Path $ResultsPath $Timestamp

New-Item -ItemType Directory -Force -Path $ResultDir | Out-Null

# ------------------------------------------------------------
# Helper: Convert native command arguments
# ------------------------------------------------------------

function ConvertTo-NativeCommandArguments {

    param(
        [Parameter(Mandatory = $true)]
        [array]$Arguments
    )

    $escapedArguments = foreach ($argument in $Arguments) {

        $arg = [string]$argument

        if ($arg -match '[\s"]') {

            '"' +
            ($arg -replace '(\\*)"', '$1$1\"' -replace '(\\+)$', '$1$1') +
            '"'

        }
        else {

            $arg

        }
    }

    return ($escapedArguments -join ' ')
}

# ------------------------------------------------------------
# Helper: Execute native process
# ------------------------------------------------------------

function Invoke-NativeProcess {

    param(
        [Parameter(Mandatory = $true)]
        [string]$FileName,

        [Parameter(Mandatory = $true)]
        [array]$Arguments
    )

    $process = New-Object System.Diagnostics.Process

    $process.StartInfo = New-Object System.Diagnostics.ProcessStartInfo

    $process.StartInfo.FileName               = $FileName
    $process.StartInfo.UseShellExecute        = $false
    $process.StartInfo.RedirectStandardOutput = $true
    $process.StartInfo.RedirectStandardError  = $true
    $process.StartInfo.CreateNoWindow         = $true

    $process.StartInfo.Arguments =
        ConvertTo-NativeCommandArguments -Arguments $Arguments

    [void]$process.Start()

    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()

    $process.WaitForExit()

    return [PSCustomObject]@{
        ExitCode = $process.ExitCode
        StdOut   = $stdout
        StdErr   = $stderr
    }
}

# ------------------------------------------------------------
# Helper: Resolve model
# ------------------------------------------------------------

function Resolve-BenchmarkModel {

    param(
        [Parameter(Mandatory = $true)]
        $Experiment,

        [Parameter(Mandatory = $true)]
        $Models
    )

    if (-not $Experiment.model) {
        return $null
    }

    $modelReference = [string]$Experiment.model

    # --------------------------------------------------------
    # 1. Match model ID
    # --------------------------------------------------------

    $model = @(
        $Models | Where-Object {
            $_.id -and
            ([string]$_.id -eq $modelReference)
        }
    ) | Select-Object -First 1

    if ($model) {
        return $model
    }

    # --------------------------------------------------------
    # 2. Match model name
    # --------------------------------------------------------

    $model = @(
        $Models | Where-Object {
            $_.name -and
            ([string]$_.name -eq $modelReference)
        }
    ) | Select-Object -First 1

    if ($model) {
        return $model
    }

    # --------------------------------------------------------
    # 3. Match model path
    # --------------------------------------------------------

    $model = @(
        $Models | Where-Object {
            $_.path -and
            ([string]$_.path -eq $modelReference)
        }
    ) | Select-Object -First 1

    if ($model) {
        return $model
    }

    # --------------------------------------------------------
    # 4. Treat experiment.model as a direct file path
    # --------------------------------------------------------

    if (Test-Path $modelReference) {

        return [PSCustomObject]@{
            id   = $modelReference
            name = [System.IO.Path]::GetFileNameWithoutExtension(
                $modelReference
            )
            path = $modelReference
        }
    }

    return $null
}

# ------------------------------------------------------------
# Benchmark header
# ------------------------------------------------------------

Write-Host ""
Write-Host "============================================================"
Write-Host "LOCAL LLM LIGHTWEIGHT BENCHMARK"
Write-Host "============================================================"
Write-Host ""

Write-Host "Collecting system information..."
Write-Host ""

# ------------------------------------------------------------
# GPU
# ------------------------------------------------------------

$gpuName   = "Unknown"
$gpuMemory = 0

try {

    $gpu = Get-CimInstance Win32_VideoController |
        Where-Object {
            $_.Name -match "NVIDIA"
        } |
        Select-Object -First 1

    if ($gpu) {
        $gpuName = $gpu.Name.Trim()
    }

}
catch {

    Write-Host "WARNING: Could not read GPU information."

}

# ------------------------------------------------------------
# VRAM
# ------------------------------------------------------------

try {

    $nvidiaSmi = Get-Command nvidia-smi.exe -ErrorAction SilentlyContinue

    if ($nvidiaSmi) {

        $vramOutput = & nvidia-smi.exe `
            --query-gpu=memory.total `
            --format=csv,noheader,nounits `
            2>$null

        if ($vramOutput) {

            $vramMB = [double](
                $vramOutput |
                Select-Object -First 1
            )

            $gpuMemory = [math]::Round(
                $vramMB / 1024,
                2
            )
        }
    }

}
catch {

    Write-Host "WARNING: Could not read VRAM information."

}

# ------------------------------------------------------------
# RAM
# ------------------------------------------------------------

$totalRamGB = 0

try {

    $ram = Get-CimInstance Win32_ComputerSystem

    $totalRamGB = [math]::Round(
        $ram.TotalPhysicalMemory / 1GB,
        2
    )

}
catch {

    Write-Host "WARNING: Could not read RAM information."

}

# ------------------------------------------------------------
# CPU
# ------------------------------------------------------------

$cpuName = "Unknown"

try {

    $cpu = Get-CimInstance Win32_Processor |
        Select-Object -First 1

    if ($cpu) {
        $cpuName = $cpu.Name.Trim()
    }

}
catch {

    Write-Host "WARNING: Could not read CPU information."

}

# ------------------------------------------------------------
# CUDA
# ------------------------------------------------------------

$cudaVersion = "Unknown"

try {

    $nvccOutput = & nvcc --version 2>$null

    $cudaLine = $nvccOutput |
        Select-String "release" |
        Select-Object -First 1

    if ($cudaLine) {

        if ($cudaLine.ToString() -match 'release\s+([0-9.]+)') {
            $cudaVersion = $Matches[1]
        }
        else {
            $cudaVersion = $cudaLine.ToString().Trim()
        }
    }

}
catch {

    $cudaVersion = "Unknown"

}

# ------------------------------------------------------------
# llama.cpp build information
# ------------------------------------------------------------

$buildVersion = "Unknown"

try {

    $versionResult = Invoke-NativeProcess `
        -FileName $LlamaBench `
        -Arguments @("--version")

    $versionText = (
        $versionResult.StdOut +
        $versionResult.StdErr
    ).Trim()

    if (
        $versionResult.ExitCode -eq 0 -and
        $versionText
    ) {

        $buildVersion = $versionText

    }

}
catch {

    $buildVersion = "Unknown"

}

# ------------------------------------------------------------
# System information display
# ------------------------------------------------------------

Write-Host "SYSTEM"
Write-Host "------------------------------------------------------------"
Write-Host ("GPU        : {0}" -f $gpuName)
Write-Host ("VRAM       : {0} GB" -f $gpuMemory)
Write-Host ("RAM        : {0} GB" -f $totalRamGB)
Write-Host ("CPU        : {0}" -f $cpuName)
Write-Host ("CUDA       : {0}" -f $cudaVersion)
Write-Host ("llama.cpp  : {0}" -f $buildVersion)
Write-Host ""

$experimentCount = @($config.experiments).Count

Write-Host ("Experiments : {0}" -f $experimentCount)
Write-Host ""

# ------------------------------------------------------------
# Result storage
# ------------------------------------------------------------

$allResults = @()

$completedExperiments = 0
$failedExperiments    = 0

# ------------------------------------------------------------
# Execute experiments
# ------------------------------------------------------------

foreach ($experiment in $config.experiments) {

    $experimentId = "unnamed"

    if ($experiment.id) {
        $experimentId = [string]$experiment.id
    }

    $description = ""

    if ($experiment.description) {
        $description = [string]$experiment.description
    }

    Write-Host ""
    Write-Host "============================================================"
    Write-Host ("EXPERIMENT: {0}" -f $experimentId)
    Write-Host "============================================================"
    Write-Host ""

    if ($description) {
        Write-Host $description
        Write-Host ""
    }

    # --------------------------------------------------------
    # Resolve model
    # --------------------------------------------------------

    $model = Resolve-BenchmarkModel `
        -Experiment $experiment `
        -Models $config.models

    if (-not $model) {

        Write-Host "ERROR: Could not resolve model for experiment."
        Write-Host ("Experiment : {0}" -f $experimentId)

        if ($experiment.model) {
            Write-Host ("Model value: {0}" -f $experiment.model)
        }
        else {
            Write-Host "Model value: (not specified)"
        }

        Write-Host ""
        Write-Host "Available models:"

        foreach ($availableModel in $config.models) {

            $availableId = "(no id)"

            if ($availableModel.id) {
                $availableId = [string]$availableModel.id
            }

            $availableName = "(no name)"

            if ($availableModel.name) {
                $availableName = [string]$availableModel.name
            }

            $availablePath = "(no path)"

            if ($availableModel.path) {
                $availablePath = [string]$availableModel.path
            }

            Write-Host (
                "  ID={0}" -f $availableId
            )

            Write-Host (
                "    Name={0}" -f $availableName
            )

            Write-Host (
                "    Path={0}" -f $availablePath
            )
        }

        $failedExperiments++

        continue
    }

    # --------------------------------------------------------
    # Validate model path
    # --------------------------------------------------------

    if (-not $model.path) {

        Write-Host ""
        Write-Host "ERROR: Resolved model does not contain a path."
        Write-Host ("Experiment: {0}" -f $experimentId)

        $failedExperiments++

        continue
    }

    if (-not (Test-Path $model.path)) {

        Write-Host ""
        Write-Host "ERROR: Model file not found:"
        Write-Host $model.path

        $failedExperiments++

        continue
    }

    # --------------------------------------------------------
    # Model information
    # --------------------------------------------------------

    $modelName = [string]$model.name

    if (-not $modelName) {
        $modelName = [System.IO.Path]::GetFileNameWithoutExtension(
            $model.path
        )
    }

    $modelInfo = Get-Item $model.path

    $modelSizeGB = [math]::Round(
        $modelInfo.Length / 1GB,
        2
    )

    $modelParametersB = $null

    if ($model.parameters_b) {
        $modelParametersB = [double]$model.parameters_b
    }
    elseif ($model.parameters) {
        $modelParametersB = [double]$model.parameters
    }

    # --------------------------------------------------------
    # GPU layers
    # --------------------------------------------------------

    $gpuLayers = 99

    if ($experiment.gpu_layers) {

        if ($experiment.gpu_layers -is [System.Array]) {
            $gpuLayers = $experiment.gpu_layers
        }
        else {
            $gpuLayers = @($experiment.gpu_layers)
        }

    }
    elseif ($config.gpu_layers) {

        if ($config.gpu_layers -is [System.Array]) {
            $gpuLayers = $config.gpu_layers
        }
        else {
            $gpuLayers = @($config.gpu_layers)
        }

    }
    else {

        $gpuLayers = @(99)

    }

    # --------------------------------------------------------
    # Benchmark parameters
    # --------------------------------------------------------

    $promptTokens = 512
    $generationTokens = 128
    $runs = 3

    if ($config.bench) {

        if ($config.bench.prompt_tokens) {
            $promptTokens =
                [int]$config.bench.prompt_tokens
        }

        if ($config.bench.generation_tokens) {
            $generationTokens =
                [int]$config.bench.generation_tokens
        }

        if ($config.bench.runs) {
            $runs =
                [int]$config.bench.runs
        }

    }

    if ($experiment.prompt_tokens) {
        $promptTokens =
            [int]$experiment.prompt_tokens
    }

    if ($experiment.generation_tokens) {
        $generationTokens =
            [int]$experiment.generation_tokens
    }

    if ($experiment.runs) {
        $runs =
            [int]$experiment.runs
    }

    # --------------------------------------------------------
    # Model display
    # --------------------------------------------------------

    Write-Host ""
    Write-Host "MODEL"
    Write-Host "------------------------------------------------------------"
    Write-Host ("Name       : {0}" -f $modelName)
    Write-Host ("Path       : {0}" -f $model.path)
    Write-Host ("Size       : {0} GB" -f $modelSizeGB)

    if ($modelParametersB -ne $null) {
        Write-Host ("Parameters : {0} B" -f $modelParametersB)
    }

    Write-Host ""

    # --------------------------------------------------------
    # Execute each GPU layer configuration
    # --------------------------------------------------------

    $experimentSucceeded = $false

    foreach ($ngl in $gpuLayers) {

        Write-Host ""
        Write-Host "------------------------------------------------------------"
        Write-Host (
            "Configuration: ngl={0}, prompt={1}, generation={2}" -f
            $ngl,
            $promptTokens,
            $generationTokens
        )
        Write-Host "------------------------------------------------------------"
        Write-Host ""

        # ----------------------------------------------------
        # Arguments
        # ----------------------------------------------------

        $arguments = @(
            "-m", $model.path,
            "-p", $promptTokens,
            "-n", $generationTokens,
            "-ngl", $ngl,
            "-r", $runs,
            "-o", "json"
        )

        # ----------------------------------------------------
        # Build command display
        # ----------------------------------------------------

        $displayArguments =
            ConvertTo-NativeCommandArguments `
                -Arguments $arguments

        Write-Host "Command:"
        Write-Host ""
        Write-Host (
            "{0} {1}" -f
            $LlamaBench,
            $displayArguments
        )
        Write-Host ""

        Write-Host "Running llama-bench..."
        Write-Host ""

        # ----------------------------------------------------
        # Output files
        # ----------------------------------------------------

        $safeExperimentId =
            $experimentId -replace '[\\/:*?"<>|]', '_'

        $safeModelName =
            $modelName -replace '[\\/:*?"<>|]', '_'

        $filePrefix =
            "{0}_{1}_ngl{2}" -f
            $safeExperimentId,
            $safeModelName,
            $ngl

        $stdoutFile =
            Join-Path $ResultDir "$filePrefix.stdout.log"

        $stderrFile =
            Join-Path $ResultDir "$filePrefix.stderr.log"

        $jsonFile =
            Join-Path $ResultDir "$filePrefix.json"

        # ----------------------------------------------------
        # Execute benchmark
        # ----------------------------------------------------

        try {

            $processResult = Invoke-NativeProcess `
                -FileName $LlamaBench `
                -Arguments $arguments

        }
        catch {

            Write-Host ""
            Write-Host "ERROR: Failed to start llama-bench."
            Write-Host $_

            $failedExperiments++

            continue
        }

        # ----------------------------------------------------
        # Save raw output
        # ----------------------------------------------------

        $processResult.StdOut |
            Set-Content `
                -Path $stdoutFile `
                -Encoding UTF8

        $processResult.StdErr |
            Set-Content `
                -Path $stderrFile `
                -Encoding UTF8

        # ----------------------------------------------------
        # Check exit code
        # ----------------------------------------------------

        if ($processResult.ExitCode -ne 0) {

            Write-Host ""
            Write-Host "ERROR: llama-bench failed."
            Write-Host (
                "Exit code: {0}" -f
                $processResult.ExitCode
            )

            if ($processResult.StdErr) {

                Write-Host ""
                Write-Host "stderr:"
                Write-Host $processResult.StdErr

            }

            continue
        }

        # ----------------------------------------------------
        # Parse JSON
        # ----------------------------------------------------

        try {

            $benchData =
                $processResult.StdOut |
                ConvertFrom-Json

            $benchData |
                ConvertTo-Json -Depth 30 |
                Set-Content `
                    -Path $jsonFile `
                    -Encoding UTF8

            foreach ($result in @($benchData)) {

                $backend = $result.backend

                $resultNgl = $ngl

                if ($null -ne $result.ngl) {
                    $resultNgl = $result.ngl
                }

                $tokensPerSec = $result.tps

                $stdDev = $result.stddev

                $allResults += [PSCustomObject]@{
                    Timestamp      = (Get-Date).ToString("o")
                    Experiment     = $experimentId
                    Description    = $description
                    Model          = $modelName
                    ModelSizeGB    = $modelSizeGB
                    ParametersB    = $modelParametersB
                    Backend        = $backend
                    GPU_Layers     = $resultNgl
                    PromptTokens   = $promptTokens
                    GenerationTokens = $generationTokens
                    Runs           = $runs
                    Test           = $result.test
                    TokensPerSec   = $tokensPerSec
                    StdDev         = $stdDev
                }

            }

            $experimentSucceeded = $true

            Write-Host "Completed."

        }
        catch {

            Write-Host ""
            Write-Host "WARNING: Could not parse llama-bench JSON."
            Write-Host ""
            Write-Host "Raw stdout was saved to:"
            Write-Host $stdoutFile
            Write-Host ""
            Write-Host "Raw stdout:"
            Write-Host $processResult.StdOut

        }
    }

    if ($experimentSucceeded) {
        $completedExperiments++
    }
    else {
        $failedExperiments++
    }
}

# ------------------------------------------------------------
# Results display
# ------------------------------------------------------------

Write-Host ""
Write-Host "============================================================"
Write-Host "RESULTS"
Write-Host "============================================================"
Write-Host ""

if ($allResults.Count -eq 0) {

    Write-Host "No benchmark results were collected."

}
else {

    $allResults |
        Select-Object `
            Experiment,
            Model,
            ModelSizeGB,
            ParametersB,
            Backend,
            GPU_Layers,
            Test,
            TokensPerSec,
            StdDev |
        Format-Table -AutoSize
}

# ------------------------------------------------------------
# Save CSV
# ------------------------------------------------------------

$csvFile =
    Join-Path $ResultDir "results.csv"

if ($allResults.Count -gt 0) {

    $allResults |
        Export-Csv `
            -Path $csvFile `
            -NoTypeInformation `
            -Encoding UTF8

}
else {

    "" |
        Set-Content `
            -Path $csvFile `
            -Encoding UTF8

}

# ------------------------------------------------------------
# Summary
# ------------------------------------------------------------

$summary = [PSCustomObject]@{

    timestamp = (Get-Date).ToString("o")

    system = [PSCustomObject]@{
        gpu       = $gpuName
        vram_gb   = $gpuMemory
        ram_gb    = $totalRamGB
        cpu       = $cpuName
        cuda      = $cudaVersion
        llama_cpp = $buildVersion
    }

    benchmark = [PSCustomObject]@{
        llama_bench = $LlamaBench
        config_file = $ConfigPath
    }

    statistics = [PSCustomObject]@{
        experiments_total     = $experimentCount
        experiments_completed = $completedExperiments
        experiments_failed    = $failedExperiments
        result_rows           = $allResults.Count
    }

    results = $allResults
}

$summaryFile =
    Join-Path $ResultDir "summary.json"

$summary |
    ConvertTo-Json -Depth 30 |
    Set-Content `
        -Path $summaryFile `
        -Encoding UTF8

# ------------------------------------------------------------
# Finish
# ------------------------------------------------------------

Write-Host ""
Write-Host "============================================================"
Write-Host "BENCHMARK COMPLETE"
Write-Host "============================================================"
Write-Host ""

Write-Host (
    "Experiments completed: {0}" -f
    $completedExperiments
)

Write-Host (
    "Experiments failed   : {0}" -f
    $failedExperiments
)

Write-Host ""

Write-Host "Results:"
Write-Host $ResultDir

Write-Host ""

Write-Host "Files:"
Write-Host ("  Summary : {0}" -f $summaryFile)
Write-Host ("  CSV     : {0}" -f $csvFile)

Write-Host ""