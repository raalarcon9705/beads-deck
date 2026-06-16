class BeadsDeck < Formula
  desc "Lightweight native dashboard for the beads (bd) issue tracker"
  homepage "https://github.com/raalarcon9705/beads-deck"
  url "https://github.com/raalarcon9705/beads-deck/archive/refs/tags/v0.4.0.tar.gz"
  sha256 "431ccedc493dee017a7c2d2696b6a3cc3f77c9aafcacdc363c5cba351f3a2cd3"
  license "MIT"
  head "https://github.com/raalarcon9705/beads-deck.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_path_exists bin/"beads-deck"
  end
end
