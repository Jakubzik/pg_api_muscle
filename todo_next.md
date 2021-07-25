- Pfad fk2 liefert einen Fehler aus der DB: wieso wir das durchgeleitet?
- Parameter Control:
  - Testen: was passiert bei "x-query-syntax-of-method": "GET", also z.B. Rufe Fkt auf ohne Parameter?

- Login Funktionalität

- Reload config: seemingly a spawn issue?

- Shutdown: graceful via channels?

# Done
OK - shutdown via request
OK - in main 147.142.232.252 konfigurierbar machen.
OK  - in db.rs 179 schon umgesetzt, auch bei post, patch, delete, muss es möglich sein, 
OK    ohne Parameter zu agieren.
OK  - was passiert, wenn in openapi gar kein Parameter Array angegeben ist?