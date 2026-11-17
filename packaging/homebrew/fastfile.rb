class Fastfile < Formula
  desc "Instant file search, preview, diff, and disk treemap for humans and AI agents"
  homepage "https://github.com/fabiomotta0311/fastfile"
  version "0.1.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/fabiomotta0311/fastfile/releases/download/v0.1.0/fastfile-macos-arm64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    else
      url "https://github.com/fabiomotta0311/fastfile/releases/download/v0.1.0/fastfile-macos-x86_64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    url "https://github.com/fabiomotta0311/fastfile/releases/download/v0.1.0/fastfile-linux-x86_64.tar.gz"
    sha256 "d3e77b66008d0d84743120d67497826b4f95d40975a1dcdb1e350a00a9a2d8c0"
  end

  def install
    bin.install "fastfile"
    bin.install "ff"
    bin.install "ff-ui" if File.exist?("ff-ui")
  end

  test do
    system "#{bin}/fastfile", "--help"
    system "#{bin}/ff", "--help"
  end
end
