from pathlib import Path

from aroll_autocut.cli import main


def test_cli_refuses_to_overwrite_existing_output(
    tmp_path: Path,
    capsys,
) -> None:
    input_path = tmp_path / "input.wav"
    output_path = tmp_path / "input.srt"
    output_path.write_text("existing", encoding="utf-8")

    result = main([str(input_path)])

    assert result == 2
    assert "使用 --force 覆盖" in capsys.readouterr().err
