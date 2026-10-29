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
        r.save(tmp_path / "x.png")
