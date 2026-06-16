class BeadsDeck < Formula
  desc "Lightweight native dashboard for the beads (bd) issue tracker"
  homepage "https://github.com/raalarcon9705/beads-deck"
  url "https://github.com/raalarcon9705/beads-deck/archive/refs/tags/v0.4.1.tar.gz"
  sha256 "4728ae6e1848e87a64d131395a7f0be4ed202eb9a40a1c236d11ae61460879bf"
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
