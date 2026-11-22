# 설치 파일(Setup.exe) 빌드 스크립트
#   사용법:  ./installer/build.ps1            (Cargo.toml 의 버전 사용)
#            ./installer/build.ps1 -Version 1.2.3
# 결과물: dist\ImageCleaner-Setup-<버전>.exe  +  .sha256
param([string]$Version)
# 네이티브 명령의 stderr 출력(cargo 진행 로그 등)을 오류로 취급하지 않고 종료 코드로 판단한다.
$ErrorActionPreference = 'Continue'
Set-Location (Join-Path $PSScriptRoot '..')

if (-not $Version) {
    $Version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.+)"').Matches[0].Groups[1].Value
}
Write-Host "== Image Cleaner $Version 빌드 =="

cargo build --release --locked
if ($LASTEXITCODE -ne 0) { throw 'cargo build 실패' }

$candidates = @(
    (Get-Command iscc -ErrorAction SilentlyContinue).Source,
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) }
if (-not $candidates) { throw 'Inno Setup(ISCC.exe)을 찾을 수 없습니다. https://jrsoftware.org/isinfo.php 에서 설치하세요.' }
$iscc = $candidates | Select-Object -First 1

& $iscc "/DAppVersion=$Version" "installer\ImageCleaner.iss"
if ($LASTEXITCODE -ne 0) { throw 'Inno Setup 컴파일 실패' }

$setup = "dist\ImageCleaner-Setup-$Version.exe"
$hash = (Get-FileHash $setup -Algorithm SHA256).Hash.ToLower()
"$hash  $(Split-Path $setup -Leaf)" | Set-Content "$setup.sha256" -Encoding ascii
Write-Host "완료: $setup"
Write-Host "SHA256: $hash"
