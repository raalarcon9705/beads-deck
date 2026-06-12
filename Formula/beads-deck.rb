class BeadsDeck < Formula
  desc "Lightweight native dashboard for the beads (bd) issue tracker"
  homepage "https://github.com/raalarcon9705/beads-deck"
  url "https://github.com/raalarcon9705/beads-deck/archive/refs/tags/v0.2.4.tar.gz"
  sha256 "c009c35e7a8e92d29242f4ee8a2a178be1ea3888b717ebcb757cff1816466dd0"
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
