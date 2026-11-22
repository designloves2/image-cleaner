// SPDX-License-Identifier: GPL-3.0-or-later
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Image Cleaner");
        res.set("FileDescription", "Image Cleaner - 이미지 중복 제거 및 정리");
        res.set("CompanyName", "Image Cleaner");
        let _ = res.compile();
    }
}
