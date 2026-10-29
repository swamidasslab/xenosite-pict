"""Vector PDF export (Rust svg2pdf) from Rendered and scenes."""

import xpict


def test_rendered_to_pdf_and_save(tmp_path):
    r = xpict.render(xpict.mol("CC(=O)Nc1ccc(O)cc1"))
    pdf = r.to_pdf()
    assert pdf.startswith(b"%PDF-")
    r.save(tmp_path / "apap.pdf")
    r.save(tmp_path / "apap.svg")
    assert (tmp_path / "apap.pdf").read_bytes().startswith(b"%PDF-")
    assert (tmp_path / "apap.svg").read_text().startswith("<svg")


def test_save_rejects_unknown_extension(tmp_path):
    import pytest

    r = xpict.render(xpict.mol("CCO"))
    with pytest.raises(ValueError):
        r.save(tmp_path / "x.eps")


def test_png_and_rgba_buffer(tmp_path):
    r = xpict.render(xpict.mol("c1ccc2sccc2c1"))
    png = r.to_png(scale=2)
    assert png.startswith(b"\x89PNG")
    r.save(tmp_path / "x.png", scale=1)
    assert (tmp_path / "x.png").read_bytes().startswith(b"\x89PNG")
    buf = r.to_rgba(scale=2)
    h, w, c = buf.shape
    assert c == 4 and buf.format == "B" and len(buf.tobytes()) == h * w * 4
    assert max(buf.tobytes()[3::4]) == 255
