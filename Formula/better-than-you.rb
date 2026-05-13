class BetterThanYou < Formula
  desc "CLI-first portrait battle tool for fictional AI-generated adult portraits"
  homepage "https://github.com/NomaDamas/BetterThanYou"
  version "0.8.20"
  license "MIT"
  head "https://github.com/NomaDamas/BetterThanYou.git", branch: "main"

  if OS.mac? && Hardware::CPU.arm?
    url "https://github.com/NomaDamas/BetterThanYou/releases/download/v0.8.20/better-than-you-v0.8.20-aarch64-apple-darwin.tar.gz"
    sha256 "16291dfe66b920a9fec201e9d02570bf3ad73a73aa6fd5c77546c17c23bd5c52"
  else
    url "https://github.com/NomaDamas/BetterThanYou/archive/refs/tags/v0.8.20.tar.gz"
    sha256 "ed247c5f4c1bbc8c3cbb75c08ea2063b75442bbc36dc4a01b6bb1990f5207835"

    depends_on "rust" => :build
  end

  def install
    if OS.mac? && Hardware::CPU.arm?
      bin.install "better-than-you"
    else
      system "cargo", "install", *std_cargo_args(path: ".")
    end
  end

  test do
    output = shell_output("#{bin}/better-than-you --help")
    assert_match "BetterThanYou", output
  end
end
