"""Legacy-shaped, silent test data only; never use an existing user's app-data."""
import json
from pathlib import Path

def seed_silent_data(folder, with_export=False):
    assert '.local-test' in folder.resolve().parts and not folder.exists()
    folder.mkdir(parents=True, mode=0o700)
    categories = [('unity', 'Юнити'), ('blender', 'Блендер'), ('programming', 'Программирование'), ('reading', 'Чтение'), ('other', 'Другое')]
    catalog = json.loads((Path(__file__).resolve().parent.parent / 'src/lib/sound-catalog.json').read_text())
    previous_bindings = {event: sound for event, sound in catalog['migrationDefaultsV2'].items() if event != 'menu_choice'}
    data = {
        'schemaVersion': 1, 'savedAt': 0,
        'settings': {'voices': {}, 'sfxVolume': 0, 'sfxBindings': previous_bindings, 'soundDefaultsVersion': 1},
        'timer': {'status': 'Idle', 'phase': 'Focus', 'remainingMs': 1500000, 'deadlineMs': None, 'segmentStartedAt': None, 'session': None, 'cycle': 0},
        'categories': [{'id': id, 'name': name, 'color': '#57b69c', 'archived': False} for id, name in categories],
        'selectedCategory': 'programming', 'sessions': [], 'breaks': [],
    }
    (folder / 'state.json').write_text(json.dumps(data))
    if with_export:
        vault = folder.parent.parent / 'vault'
        (vault / 'Miku Focus/Данные').mkdir(parents=True)
        (folder / 'obsidian.json').write_text(json.dumps({'schemaVersion': 1, 'sourceId': 'linux-smoke-fixture', 'folder': str(vault.resolve())}))
