- Wenn beim Start postgresql nicht angeschaltet ist, bleibt die Verbindung zur DB unmöglich
   - proof: systemctl stop postgresql
   - starte Debug
   - <https://localhost:8443/toc?buch_id=1> liefert "NoClientDbAvailable"
   - gewünscht: (1) reload sollte das beheben können? Oder "reconnect"?
   - (2) Bei dem speziellen Fehler (NoClientDbAvailable) sollte dann neu versucht werden, zu verbinden.
- Testprojekt mit Datenbank aufstellen und Testclient implementieren (node? curl?)
- allow for =eq. - Syntax
- Pfad fk2 liefert einen Fehler aus der DB: wieso wir das durchgeleitet?
- Parameter Control:
  - Testen: was passiert bei "x-query-syntax-of-method": "GET", also z.B. Rufe Fkt auf ohne Parameter?

- Login Funktionalität

- Reload config: seemingly a spawn issue?

- Shutdown: graceful via channels?

# Done

OK - Fehlern bei static "not found" fehlt das letzte Zeichen (proof: <https://localhost:8443/static/sf/kapitel?kapitel_id=eq.64>)
OK - Javascript Datei Auslieferung (auhc html?): fehlt letztes Zeichen?
OK - shutdown via request
OK - in main 147.142.232.252 konfigurierbar machen.
OK  - in db.rs 179 schon umgesetzt, auch bei post, patch, delete, muss es möglich sein, 
OK    ohne Parameter zu agieren.
OK  - was passiert, wenn in openapi gar kein Parameter Array angegeben ist?