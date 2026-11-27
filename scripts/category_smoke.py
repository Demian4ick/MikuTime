"""Native create/delete regression, called only by the PID-scoped private runner."""
import time
import stat


def run(app, *, data, click, find, keys, entry, entry_text, disabled, state, persisted, check, all_text, screenshot):
    def categories():
        click('Настройки'); click('Категории')

    def visible(s):
        return [c for c in s['categories'] if not c.get('deleted')]

    before = state()
    categories()
    click('+ Создать')
    check('empty submission explains the missing name without creating a category', 'Сначала придумайте название категории.' in all_text(app) and len(visible(state())) == len(visible(before)))
    name_input = find(app, lambda o: o.get_role_name() == 'entry' and o.get_name() == 'Новая категория')
    from gi.repository import Atspi
    check('empty submission focuses the name field', name_input is not None and name_input.get_state_set().contains(Atspi.StateType.FOCUSED))
    screenshot('category-name-hint.png')
    entry('Новая категория', '   ')
    click('+ Создать')
    check('whitespace submission keeps the hint and does not save', 'Сначала придумайте название категории.' in all_text(app) and len(visible(state())) == len(visible(before)))
    entry('Новая категория', 'Smoke Category')
    check('typing a name clears the error', 'Сначала придумайте название категории.' not in all_text(app) and not disabled('+ Создать'))
    time.sleep(1.2)
    check('clock snapshots preserve the new category draft', entry_text('Новая категория') == 'Smoke Category')
    create = find(app, lambda o: o.get_name() == '+ Создать' and o.get_action_iface() is not None)
    assert create is not None and create.get_action_iface().do_action(0)
    # AT-SPI correctly refuses further actions once the submit button is disabled.
    for _ in range(4): create.get_action_iface().do_action(0)
    assert persisted(lambda s: len(visible(s)) == len(visible(before)) + 1)
    matches = [c for c in visible(state()) if c['name'] == 'Smoke Category']
    check('rapid create clicks add exactly one category', len(matches) == 1 and entry_text('Новая категория') == '')
    created = matches[0]
    click('Удалить категорию Smoke Category')
    check('delete requires explicit confirmation', not next(c for c in state()['categories'] if c['id'] == created['id']).get('deleted'))
    confirm = find(app, lambda o: o.get_name() == 'Да, удалить')
    keys(confirm, 'Escape')
    check('Escape cancels deletion', find(app, lambda o: o.get_name() == 'Да, удалить') is None)
    click('Удалить категорию Smoke Category'); click('Отмена')
    check('cancel leaves category unchanged', next(c for c in visible(state()) if c['id'] == created['id']) == created)
    click('В архив: Smoke Category')
    check('created categories can be archived', persisted(lambda s: next(c for c in s['categories'] if c['id'] == created['id'])['archived']))
    click('Вернуть из архива: Smoke Category')
    check('archived categories can be restored', persisted(lambda s: not next(c for c in s['categories'] if c['id'] == created['id'])['archived']))
    click('♡ Комната фокуса'); click('Выбрать категорию'); click('Smoke Category')
    check('new category is selectable', persisted(lambda s: s['selectedCategory'] == created['id']))
    click('▶ Начать работу'); time.sleep(.5); click('Ⅱ Пауза')
    checkpoint = state()
    categories()
    screenshot('categories.png')
    click('Удалить категорию Smoke Category')
    mode = stat.S_IMODE(data.stat().st_mode)
    try:
        data.chmod(0o500)  # Only this runner's private directory; never the user's data.
        click('Да, удалить'); time.sleep(1.2)
        check('failed disk write keeps the category visible and undeleted', find(app, lambda o: o.get_name() == 'Удалить категорию Smoke Category') is not None and not next(c for c in state()['categories'] if c['id'] == created['id']).get('deleted'))
    finally:
        data.chmod(mode)
    click('Да, удалить')
    assert persisted(lambda s: next(c for c in s['categories'] if c['id'] == created['id']).get('deleted'))
    current = state()
    check('deletion preserves paused session, timestamps and all history', all(current[key] == checkpoint[key] for key in ['timer', 'sessions', 'breaks']))
    deleted = next(c for c in current['categories'] if c['id'] == created['id'])
    check('deleted metadata preserves name and color', deleted['name'] == created['name'] and deleted['color'] == created['color'])
    check('deletion removes the editor and chooses an active fallback', find(app, lambda o: o.get_name() == 'Удалить категорию Smoke Category') is None and current['selectedCategory'] != created['id'])
    click('♡ Комната фокуса'); click('Выбрать категорию')
    check('deleted category is absent from the focus menu', find(app, lambda o: o.get_name() == 'Smoke Category') is None)
    keys(find(app, lambda o: o.get_name() == 'Выбрать категорию'), 'Escape')
    click('✓ Завершить')
    check('finishing keeps the original deleted category ID', persisted(lambda s: len(s['sessions']) == 1 and s['sessions'][0]['categoryId'] == created['id']))
    click('Закончить отдых »'); click('✧ Дневник занятий')
    check('diary retains the deleted category label', 'Smoke Category' in all_text(app))
    categories()
    entry('Новая категория', 'Smoke Category'); click('+ Создать')
    assert persisted(lambda s: any(c['name'] == 'Smoke Category' for c in visible(s)))
    replacement = next(c for c in visible(state()) if c['name'] == 'Smoke Category')
    check('recreating the same name does not reuse historical IDs', replacement['id'] != created['id'])
    click('Удалить категорию Smoke Category'); click('Да, удалить')
    for c in list(visible(state()))[1:]:
        click('Удалить категорию ' + c['name']); click('Да, удалить')
    remaining = visible(state())
    check('last active category cannot be deleted or archived', len(remaining) == 1 and disabled('Удалить категорию ' + remaining[0]['name']) and disabled('В архив: ' + remaining[0]['name']))
    saved_history = state()['sessions']
    click('Сохранить и выйти')
    check('exit preserves history and deleted category metadata', state()['sessions'] == saved_history and next(c for c in state()['categories'] if c['id'] == created['id'])['deleted'])
