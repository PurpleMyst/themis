from pathlib import Path
from subprocess import run

from send2trash import send2trash
from tqdm import tqdm


def main() -> None:
    files = list(Path("tiles").glob("*.heic"))
    with tqdm(files) as pbar:
        for file in pbar:
            pbar.set_description(f"Converting {file}")
            run(("magick", file, file.with_suffix(".jpg")))
            send2trash(file)


if __name__ == "__main__":
    main()
