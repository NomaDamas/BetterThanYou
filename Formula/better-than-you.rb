class BetterThanYou < Formula
  desc "CLI-first portrait battle tool for fictional AI-generated adult portraits"
  homepage "https://github.com/NomaDamas/BetterThanYou"
  url "https://github.com/NomaDamas/BetterThanYou/archive/refs/tags/v0.4.0.tar.gz"
  version "0.4.0"
  sha256 "d4335d868b916a4715ce6aa9cae54b472be3613d30d34e2b539dc0a3a57791c2"
  license "MIT"
  head "https://github.com/NomaDamas/BetterThanYou.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    output = shell_output("#{bin}/better-than-you --help")
    assert_match "BetterThanYou", output
  end
end
