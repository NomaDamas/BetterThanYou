class BetterThanYou < Formula
  desc "CLI-first portrait battle tool for fictional AI-generated adult portraits"
  homepage "https://github.com/NomaDamas/BetterThanYou"
  version "0.8.17"
  license "MIT"
  head "https://github.com/NomaDamas/BetterThanYou.git", branch: "main"

  if OS.mac? && Hardware::CPU.arm?
    url "https://github.com/NomaDamas/BetterThanYou/releases/download/v0.8.17/better-than-you-v0.8.17-aarch64-apple-darwin.tar.gz"
    sha256 "6ca2995df044a46f2def76ebeb825115fe27948a3e0255b3766547a992c04e36"
  else
    url "https://github.com/NomaDamas/BetterThanYou/archive/refs/tags/v0.8.17.tar.gz"
    sha256 "bda9289cbea026391267a9e2d32af350c52ffa4629b7f38a16cf0b809eb4b74f"

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
