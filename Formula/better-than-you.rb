class BetterThanYou < Formula
  desc "CLI-first portrait battle tool for fictional AI-generated adult portraits"
  homepage "https://github.com/NomaDamas/BetterThanYou"
  url "https://github.com/NomaDamas/BetterThanYou/archive/refs/heads/main.tar.gz"
  version "0.2.0"
  sha256 "fd7fe2662d40c371368481343aba21963113316a3e1060b806e4d901bbbf45cf"
  license "MIT"
  head "https://github.com/NomaDamas/BetterThanYou.git", branch: "main"

  depends_on "node"

  def install
    libexec.install "packages"

    (libexec/"packages/cli/node_modules/@better-than-you").mkpath
    cp_r libexec/"packages/core", libexec/"packages/cli/node_modules/@better-than-you/core"

    system "npm", "install", "--prefix", libexec, "jimp@^1.6.0"

    (bin/"better-than-you").write <<~SH
      #!/bin/bash
      export NODE_PATH="#{libexec}/node_modules"
      exec "#{Formula["node"].opt_bin}/node" "#{libexec}/packages/cli/bin/better-than-you.js" "$@"
    SH
  end

  test do
    output = shell_output("#{bin}/better-than-you --help")
    assert_match "BetterThanYou", output
  end
end
