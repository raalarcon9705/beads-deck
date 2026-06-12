class BeadsDeck < Formula
  desc "Lightweight native dashboard for the beads (bd) issue tracker"
  homepage "https://github.com/raalarcon9705/beads-deck"
  url "https://github.com/raalarcon9705/beads-deck/archive/refs/tags/v0.2.3.tar.gz"
  sha256 "4c0e98c110b65ddb16964c7f791503876196f8b6f1bb52c7d2a3a965db9e1263"
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
