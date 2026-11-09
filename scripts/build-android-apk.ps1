param(
    [ValidateSet("release", "debug")]
    [string]$Profile = "release"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $RepoRoot

$SdkRoot = $env:ANDROID_HOME
if (-not $SdkRoot -or -not (Test-Path $SdkRoot)) {
    $SdkRoot = $env:ANDROID_SDK_ROOT
}
if (-not $SdkRoot -or -not (Test-Path $SdkRoot)) {
    $SdkRoot = Join-Path $env:LOCALAPPDATA "Android\Sdk"
}
if (-not (Test-Path $SdkRoot)) {
    throw "Android SDK not found. Set ANDROID_HOME or install Android SDK under $env:LOCALAPPDATA\Android\Sdk."
}

$NdkDir = if ($env:ANDROID_NDK_HOME -and (Test-Path $env:ANDROID_NDK_HOME)) {
    $env:ANDROID_NDK_HOME
} else {
    Get-ChildItem -Path (Join-Path $SdkRoot "ndk") -Directory |
        Sort-Object {
            $ver = $null
            if ([version]::TryParse($_.Name, [ref]$ver)) { $ver } else { [version]"0.0" }
        } -Descending |
        Select-Object -ExpandProperty FullName -First 1
}
if (-not $NdkDir) {
    throw "Android NDK not found under $SdkRoot\ndk."
}

$BuildToolsDir = if ($env:ANDROID_BUILD_TOOLS_DIR -and (Test-Path $env:ANDROID_BUILD_TOOLS_DIR)) {
    $env:ANDROID_BUILD_TOOLS_DIR
} else {
    Get-ChildItem -Path (Join-Path $SdkRoot "build-tools") -Directory |
        Sort-Object {
            $ver = $null
            if ([version]::TryParse($_.Name, [ref]$ver)) { $ver } else { [version]"0.0" }
        } -Descending |
        Select-Object -ExpandProperty FullName -First 1
}
if (-not $BuildToolsDir) {
    throw "Android build-tools not found under $SdkRoot\build-tools."
}

$PlatformDir = if ($env:ANDROID_PLATFORM_DIR -and (Test-Path $env:ANDROID_PLATFORM_DIR)) {
    $env:ANDROID_PLATFORM_DIR
} else {
    Get-ChildItem -Path (Join-Path $SdkRoot "platforms") -Directory |
        Where-Object { $_.Name -match '^android-(\d+)$' } |
        Sort-Object { [int]([regex]::Match($_.Name, '^android-(\d+)$').Groups[1].Value) } -Descending |
        Select-Object -ExpandProperty FullName -First 1
}
if (-not $PlatformDir) {
    throw "Android platform not found under $SdkRoot\platforms."
}
$AndroidJar = Join-Path $PlatformDir "android.jar"
if (-not (Test-Path $AndroidJar)) {
    throw "android.jar not found at $AndroidJar."
}

$NdkBin = Join-Path $NdkDir "toolchains\llvm\prebuilt\windows-x86_64\bin"
$Clang = Join-Path $NdkBin "aarch64-linux-android28-clang.cmd"
$ClangPP = Join-Path $NdkBin "aarch64-linux-android28-clang++.cmd"
$LlvmAr = Join-Path $NdkBin "llvm-ar.exe"
$LlvmStrip = Join-Path $NdkBin "llvm-strip.exe"
$ReadObj = Join-Path $NdkBin "llvm-readobj.exe"

$Aapt2 = Join-Path $BuildToolsDir "aapt2.exe"
$D8 = Join-Path $BuildToolsDir "d8.bat"
$ZipAlign = Join-Path $BuildToolsDir "zipalign.exe"
$ApkSigner = Join-Path $BuildToolsDir "apksigner.bat"

$env:ANDROID_HOME = $SdkRoot
$env:ANDROID_NDK_HOME = $NdkDir
$env:CC_aarch64_linux_android = $Clang
$env:CXX_aarch64_linux_android = $ClangPP
$env:AR_aarch64_linux_android = $LlvmAr
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $Clang
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS = "-C link-arg=-Wl,-z,max-page-size=16384"

Write-Host "[1/6] Building native Rust libraries (aarch64-linux-android, $Profile)..."
$CargoArgs = @("build", "--target", "aarch64-linux-android", "--lib", "-p", "thk-android-ffi", "-p", "thk-app")
if ($Profile -eq "release") {
    $CargoArgs += "--release"
}
& cargo @CargoArgs
if ($LASTEXITCODE -ne 0) {
    throw "cargo build failed with exit code $LASTEXITCODE"
}

$CargoOutDir = if ($Profile -eq "release") { "release" } else { "debug" }
$SoPath = Join-Path $RepoRoot "target\aarch64-linux-android\$CargoOutDir\libthk_app.so"
if (-not (Test-Path $SoPath)) {
    throw "Expected native shared library missing at $SoPath"
}
$FfiSoPath = Join-Path $RepoRoot "target\aarch64-linux-android\$CargoOutDir\libthk_android_ffi.so"
if (-not (Test-Path $FfiSoPath)) {
    throw "Expected native shared library missing at $FfiSoPath"
}

Write-Host "[2/6] Auditing ELF dependencies and 16 KB page alignment..."
$NeedsCppShared = $false
foreach ($ElfPath in @($SoPath, $FfiSoPath)) {
    $NeededOutput = (& $ReadObj --needed-libs $ElfPath) -join "`n"
    Write-Host $NeededOutput
    if ($NeededOutput -match "libc\+\+_shared\.so") {
        $NeedsCppShared = $true
    }

    $PhdrOutput = (& $ReadObj --program-headers $ElfPath) -join "`n"
    $LoadAlignments = [regex]::Matches($PhdrOutput, "Type:\s+PT_LOAD[\s\S]*?Alignment:\s+(\d+)")
    if ($LoadAlignments.Count -eq 0) {
        throw "Failed to inspect PT_LOAD segments in $ElfPath"
    }
    foreach ($m in $LoadAlignments) {
        $alignVal = [int]$m.Groups[1].Value
        if ($alignVal -lt 16384) {
            throw "PT_LOAD segment alignment ($alignVal) in $ElfPath is below 16384 bytes"
        }
    }
}

$AndroidOutRoot = Join-Path $RepoRoot "target\android"
$StageDir = Join-Path $AndroidOutRoot "stage"
if (Test-Path $StageDir) {
    Remove-Item -Path $StageDir -Recurse -Force
}
$ResDir = Join-Path $StageDir "res"
$ValuesDir = Join-Path $ResDir "values"
$MipmapDir = Join-Path $ResDir "mipmap-xxxhdpi"
$GenDir = Join-Path $StageDir "gen"
$ClassesDir = Join-Path $StageDir "classes"
$DexDir = Join-Path $StageDir "dex"
$AssetsStageDir = Join-Path $StageDir "assets"
New-Item -ItemType Directory -Force -Path $ValuesDir, $MipmapDir, $GenDir, $ClassesDir, $DexDir, $AssetsStageDir | Out-Null

Copy-Item -Path (Join-Path $RepoRoot "android\res\values\strings.xml") -Destination (Join-Path $ValuesDir "strings.xml") -Force
Copy-Item -Path (Join-Path $RepoRoot "assets\icons\icon.png") -Destination (Join-Path $MipmapDir "ic_launcher.png") -Force

$RuntimeZipSrc = Join-Path $RepoRoot "android\assets\mobile_runtime_arm64.zip"
if (-not (Test-Path $RuntimeZipSrc)) {
    throw "Missing Android mobile runtime bundle at $RuntimeZipSrc"
}
Copy-Item -Path $RuntimeZipSrc -Destination (Join-Path $AssetsStageDir "mobile_runtime_arm64.zip") -Force

Write-Host "[3/6] Compiling Android resources and linking manifest with aapt2..."
$CompiledResZip = Join-Path $StageDir "compiled_res.zip"
$BaseApk = Join-Path $StageDir "base.unaligned.apk"
$StagedManifest = Join-Path $StageDir "AndroidManifest.xml"
$ManifestContent = Get-Content -Raw -Encoding UTF8 -Path (Join-Path $RepoRoot "android\AndroidManifest.xml")
if ($Profile -eq "debug" -and $ManifestContent -notmatch 'android:debuggable=') {
    $ManifestContent = $ManifestContent -replace '<application', '<application android:debuggable="true"'
}
[System.IO.File]::WriteAllText($StagedManifest, $ManifestContent, [System.Text.UTF8Encoding]::new($false))
& $Aapt2 compile --dir $ResDir -o $CompiledResZip
if ($LASTEXITCODE -ne 0) {
    throw "aapt2 compile failed"
}
& $Aapt2 link -o $BaseApk --manifest $StagedManifest -I $AndroidJar -A $AssetsStageDir -0 .zip --java $GenDir $CompiledResZip
if ($LASTEXITCODE -ne 0) {
    throw "aapt2 link failed"
}

Write-Host "[4/6] Compiling MainActivity.java and DEX bytecode..."
$JavaSources = @()
$JavaSources += Get-ChildItem -Path (Join-Path $RepoRoot "android\java") -Filter "*.java" -Recurse | Select-Object -ExpandProperty FullName
$JavaSources += Get-ChildItem -Path $GenDir -Filter "*.java" -Recurse | Select-Object -ExpandProperty FullName

& javac --release 11 -encoding UTF-8 -Xlint:all -Werror -cp $AndroidJar -d $ClassesDir @JavaSources
if ($LASTEXITCODE -ne 0) {
    throw "javac failed"
}

$ClassFiles = Get-ChildItem -Path $ClassesDir -Filter "*.class" -Recurse | Select-Object -ExpandProperty FullName
& $D8 --min-api 28 --lib $AndroidJar --output $DexDir @ClassFiles
if ($LASTEXITCODE -ne 0) {
    throw "d8 failed"
}
$DexFile = Join-Path $DexDir "classes.dex"
if (-not (Test-Path $DexFile)) {
    throw "classes.dex was not generated"
}

Write-Host "[5/6] Packaging DEX and uncompressed (STORED method 0) arm64-v8a native libraries..."
$ApkRootStage = Join-Path $StageDir "apk_add"
$NativeAbiStage = Join-Path $ApkRootStage "lib\arm64-v8a"
New-Item -ItemType Directory -Force -Path $NativeAbiStage | Out-Null
$StagedSo = Join-Path $NativeAbiStage "libthk_app.so"
Copy-Item -Path $SoPath -Destination $StagedSo -Force
if ($Profile -eq "debug" -and (Test-Path $LlvmStrip)) {
    & $LlvmStrip --strip-debug $StagedSo
}

$StagedFfiSo = Join-Path $NativeAbiStage "libthk_android_ffi.so"
Copy-Item -Path $FfiSoPath -Destination $StagedFfiSo -Force
if ($Profile -eq "debug" -and (Test-Path $LlvmStrip)) {
    & $LlvmStrip --strip-debug $StagedFfiSo
}

if ($NeedsCppShared) {
    $CppShared = Join-Path $NdkDir "toolchains\llvm\prebuilt\windows-x86_64\sysroot\usr\lib\aarch64-linux-android\libc++_shared.so"
    if (-not (Test-Path $CppShared)) {
        throw "libc++_shared.so required by binary but not found at $CppShared"
    }
    Copy-Item -Path $CppShared -Destination (Join-Path $NativeAbiStage "libc++_shared.so") -Force
}

& jar uf $BaseApk -C $DexDir "classes.dex"
if ($LASTEXITCODE -ne 0) {
    throw "jar uf classes.dex failed"
}

& jar uf0 $BaseApk -C $ApkRootStage "lib/arm64-v8a"
if ($LASTEXITCODE -ne 0) {
    throw "jar uf0 lib/arm64-v8a failed"
}

$AlignedApk = Join-Path $StageDir "aligned.apk"
& $ZipAlign -P 16 -f 4 $BaseApk $AlignedApk
if ($LASTEXITCODE -ne 0) {
    throw "zipalign failed"
}

Write-Host "[6/6] Signing and verifying 16 KB page-aligned APK..."
$KeystorePath = Join-Path $env:USERPROFILE ".android\debug.keystore"
$StorePass = "android"
$KeyAlias = "androiddebugkey"
$KeyPass = "android"

if (-not (Test-Path $KeystorePath)) {
    $KeystorePath = Join-Path $AndroidOutRoot "thk-signing.jks"
    $StorePass = "thklaucher2026"
    $KeyAlias = "thklaucher"
    $KeyPass = "thklaucher2026"
    if (-not (Test-Path $KeystorePath)) {
        & keytool -genkeypair -v `
            -keystore $KeystorePath `
            -storepass $StorePass `
            -keypass $KeyPass `
            -alias $KeyAlias `
            -keyalg RSA `
            -keysize 2048 `
            -validity 10000 `
            -dname "CN=TriHienKun Laucher, O=TriHienKun, C=VN"
        if ($LASTEXITCODE -ne 0) {
            throw "keytool failed to generate signing keystore"
        }
    }
}

$OutApkName = "trihienkun-laucher-arm64-v8a-$Profile.apk"
$OutApk = Join-Path $AndroidOutRoot $OutApkName
& $ApkSigner sign `
    --v1-signing-enabled true `
    --v2-signing-enabled true `
    --v3-signing-enabled true `
    --ks $KeystorePath `
    --ks-pass "pass:$StorePass" `
    --ks-key-alias $KeyAlias `
    --key-pass "pass:$KeyPass" `
    --out $OutApk `
    $AlignedApk
if ($LASTEXITCODE -ne 0) {
    throw "apksigner sign failed"
}

$AlignCheck = (& $ZipAlign -c -v -P 16 4 $OutApk) -join "`n"
Write-Host $AlignCheck
if ($LASTEXITCODE -ne 0) {
    throw "zipalign 16 KB verification failed on $OutApk"
}
if ($AlignCheck -match "libthk_(app|android_ffi)\.so \(OK - compressed\)") {
    throw "Native .so libraries must be stored uncompressed (ZIP method 0) for extractNativeLibs=false"
}

& $ApkSigner verify --verbose $OutApk
if ($LASTEXITCODE -ne 0) {
    throw "apksigner verify failed on $OutApk"
}

$ApkSizeMb = [math]::Round((Get-Item $OutApk).Length / 1MB, 2)
Write-Host "SUCCESS: $OutApk ($ApkSizeMb MB)"
