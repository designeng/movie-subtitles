# Movie Subtitles

Tauri 2 + Vue 3 + TypeScript приложение: скачивает фильм с YouTube / VK Video / OK.ru,
находит субтитры (OpenSubtitles.com, SubDL.com), показывает их поверх плеера и позволяет
вручную сдвигать таймлайн субтитров. Есть перевод субтитров (Google Cloud Translation).

## Запуск

```sh
brew install ffmpeg      # нужен для склейки видео- и аудиодорожек
pnpm install
pnpm tauri dev
```

`yt-dlp` скачивается автоматически при первом использовании: зафиксированная
версия, проверка SHA-256 (см. `src-tauri/src/tools.rs`). Свой бинарник можно
указать через `MOVIE_SUBTITLES_YT_DLP=/path/to/yt-dlp`.

Ключи API (OpenSubtitles, SubDL, Google Translate) и папка для фильмов задаются в Settings.
По умолчанию фильмы сохраняются в `~/Library/Application Support/com.savenok.moviesubtitles/videos`.

## Горячие клавиши

| Клавиша | Действие |
|---|---|
| `G` / `H` | субтитры раньше / позже на 0.1 с (`Shift` — 1 с) |
| `Space` | play / pause |
| `←` / `→` | перемотка ±5 с |
| `T` | оригинал → оба → перевод |
| `F` / `Esc` | полноэкранный режим |

В списке строк кнопка ⏱ выравнивает все субтитры по строке, которая звучит сейчас.

## Структура

- `src-tauri/src/video/` — провайдеры видео (`VideoProvider`), сейчас yt-dlp для YouTube, VK и OK.ru
- `src-tauri/src/subtitles/` — провайдеры субтитров (`SubtitleProvider`), парсер SRT/VTT
- `src-tauri/src/translate.rs` — перевод (`Translator`)
- `src-tauri/src/library.rs` — библиотека фильмов; удаление чистит видео, фрагменты, субтитры и переводы
- Данные: `~/Library/Application Support/com.savenok.moviesubtitles/`
