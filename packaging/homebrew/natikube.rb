# TEMPLATE, not published yet. The README lists Homebrew as "coming soon".
# Before publishing: set `version` and replace the sha256 below with the hash
# from the release's NatiKube-<version>-macos-arm64.dmg.sha256 file.
cask "natikube" do
  version "0.1.0"
  sha256 "REPLACE_WITH_SHA256_FROM_RELEASE_DMG_SHA256_FILE" # placeholder, not a real hash

  url "https://github.com/paulpwo/NatiKube/releases/download/v#{version}/NatiKube-#{version}-macos-arm64.dmg"
  name "NatiKube"
  desc "Native Kubernetes desktop client for macOS"
  homepage "https://github.com/paulpwo/NatiKube"

  depends_on arch: :arm64
  depends_on macos: ">= :ventura"

  app "NatiKube.app"

  zap trash: [
    "~/Library/Application Support/NatiKube",
    "~/Library/Preferences/dev.natikube.app.plist",
  ]
end
