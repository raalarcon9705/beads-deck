class BeadsDeck < Formula
  desc "Lightweight native dashboard for the beads (bd) issue tracker"
  homepage "https://github.com/raalarcon9705/beads-deck"
  url "https://github.com/raalarcon9705/beads-deck/archive/refs/tags/v0.4.2.tar.gz"
  sha256 "b6e89205c40d746f30f31b115059cddf286b1448358d282e18b39917b63fd058"
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
