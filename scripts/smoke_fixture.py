"""Legacy-shaped, silent test data only; never use an existing user's app-data."""
import json

def seed_silent_data(folder):
    assert '.local-test' in folder.resolve().parts and not folder.exists()
    folder.mkdir(parents=True, mode=0o700)
    categories = [('unity', 'Юнити'), ('blender', 'Блендер'), ('programming', 'Программирование'), ('reading', 'Чтение'), ('other', 'Другое')]
    data = {
        'schemaVersion': 1, 'savedAt': 0,
        'settings': {'sfxVolume': 0, 'sfxBindings': {}},
        'timer': {'status': 'Idle', 'phase': 'Focus', 'remainingMs': 1500000, 'deadlineMs': None, 'segmentStartedAt': None, 'session': None, 'cycle': 0},
        'categories': [{'id': id, 'name': name, 'color': '#57b69c', 'archived': False} for id, name in categories],
        'selectedCategory': 'programming', 'sessions': [], 'breaks': [],
    }
    (folder / 'state.json').write_text(json.dumps(data))
