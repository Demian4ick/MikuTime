"""Copy user-supplied art; never modify originals. Pillow only needed for app icon."""
from pathlib import Path
from shutil import copy2
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parent.parent
dest = root / 'public' / 'sprites'
dest.mkdir(parents=True, exist_ok=True)
for source in (root / 'Sprites').glob('*.png'):
    copy2(source, dest / source.name)
copy2(root / 'background.jpg', root / 'public' / 'background.jpg')
icons = root / 'src-tauri' / 'icons'
icons.mkdir(parents=True, exist_ok=True)
im = Image.new('RGBA', (256, 256), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
d.rounded_rectangle((8, 8, 248, 248), 54, fill='#159eaa', outline='#c9ffec', width=8)
d.ellipse((58, 57, 198, 197), fill='#e9fff6', outline='#075666', width=6)
d.line((128, 78, 128, 131, 163, 150), fill='#159eaa', width=10)
d.ellipse((116, 119, 140, 143), fill='#eb87b5')
im.save(icons / 'icon.png')
im.save(icons / 'icon.ico', sizes=[(32, 32), (48, 48), (128, 128), (256, 256)])
