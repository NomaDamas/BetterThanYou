class BetterThanYou < Formula
  desc "CLI-first portrait battle tool for fictional AI-generated adult portraits"
  homepage "https://github.com/NomaDamas/BetterThanYou"
  version "0.8.18"
  license "MIT"
  head "https://github.com/NomaDamas/BetterThanYou.git", branch: "main"

  if OS.mac? && Hardware::CPU.arm?
    url "https://github.com/NomaDamas/BetterThanYou/releases/download/v0.8.18/better-than-you-v0.8.18-aarch64-apple-darwin.tar.gz"
    sha256 "aa20022c2c562c2730f1763f979405478efd5883ae80159ef847cb9be78b0f4e"
  else
    url "https://github.com/NomaDamas/BetterThanYou/archive/refs/tags/v0.8.18.tar.gz"
    sha256 "0bff27a6a73e08c9b8255e8f7b8832a4da23013aaed55766c884251512e1cec9"

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
