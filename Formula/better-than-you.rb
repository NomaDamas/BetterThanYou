class BetterThanYou < Formula
  desc "CLI-first portrait battle tool for fictional AI-generated adult portraits"
  homepage "https://github.com/NomaDamas/better-than-you"
  url "https://github.com/NomaDamas/better-than-you/archive/refs/heads/main.tar.gz"
  version "0.2.0"
  sha256 :no_check
  license "MIT"
  head "https://github.com/NomaDamas/better-than-you.git", branch: "main"

  depends_on "node"

  def install
    system "npm", "install", "--include-workspace-root", "--workspaces", "--omit=dev"

    libexec.install Dir["*"]

    (bin/"better-than-you").write_env_script libexec/"packages/cli/bin/better-than-you.js", {
      "NODE_PATH" => libexec/"node_modules"
    }
  end

  test do
    output = shell_output("#{bin}/better-than-you --help")
    assert_match "BetterThanYou", output
  end
end
