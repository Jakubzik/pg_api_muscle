- Pfad zu den statischen Dateien relativieren (also: den 'echten' Pfad konfigurierbar machen!)
- Cacheing mit HashMap
- Testen: client-ip restriction, Claims, more than one context funnind
- Documentation
- Wo ist der Code für API reload gelandet?
- Document and Test: ClaimItems etc.? (See API)
- try Urlaub
- try db minutes
- try SignUp replacement?
- Post-Parameter: maxlen (auch sf_test.js wieder aktivieren: Login-Name begrenzen auf 80 Zeichen)
- Ermöglichen, dass überflüssige Parameter (Post-Parameter insbes.) zum ERR führen (in muscle.ini)
- Wenn beim Start postgresql nicht angeschaltet ist, bleibt die Verbindung zur DB unmöglich
   - proof: systemctl stop postgresql
   - starte Debug
   - <https://localhost:8443/toc?buch_id=1> liefert "NoClientDbAvailable"
   - gewünscht: (1) reload sollte das beheben können? Oder "reconnect"?
   - (2) Bei dem speziellen Fehler (NoClientDbAvailable) sollte dann neu versucht werden, zu verbinden.
- Configure Size limit for a request (denial of service)
- Response: header configurierbar (Allow Access Cross Origin etc.)
- Cookies?
- Test für lt. etc. Überhaupt: größere Test-Suite mit Todo o.ä.
- Restriction auf Client IP mit Wildcard und/oder Liste von IPs
- Überlegen: sollte use_extended_url-Syntax in der API pro Endpunkt konfigurierbar sein?
- Überlegen: Fehlermeldungsseiten pro endpoint definieren?
- Überlegen: sollten auch Array und Object-Typen per API konfigurierbar sein? Und überprüfbar? Und weiterleitbar an die DB?
- Ermöglichen eines Pfads/Prefixes für dynamische Anfragen (analog zu ./static).
- Ponder Design question again: parse only those parts of the request that are part of the API specification? (Rather than 
  analyse query string and payload and parse it all into parameters with names and values -- only parse what is part 
  of the specification?)
- do we need more than one static folder? And/or static folder alias and actual representation?
- Testprojekt mit Datenbank aufstellen und Testclient implementieren (node? curl?)
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
OK Für statische Seiten im Moment hartkodiert /static/ -- das muss in muscle.ini konfigurierbar sein.
OK - allow for =eq. - Syntax
  -> Ziel: https://localhost:8443/static/sf/test.html soll wieder laufen
  - api.check_query_parameters braucht Reaktion auf conf.use_eq_syntax_on_url_parameters:
      - (1) Checked-Parameter braucht ein Feld für Relation (String, bzw. "=", "!=", "<", ">", "<=", ">=", "~=", "IN")
      - (2) Falls use_eq_syntax_on_url_parameters=true muss die Relation beim Check gesetzt werden
OK TOC Update: irgendwie ist der Parameter "toc" leer. Why? (2021-9-13)
OK db.get_parameter_where_criteria braucht Variable anstatt "="
OK Bei Start output: auf welchem Port lausche ich? HTTP(S)?
OK Teste Restriction auf Client IP
OK Erlaube http (lokal) anstatt https. Oder sogar socket? HTTP sieht gut aus: Der Unterschied in main ist BLOSS in 481/482. die if-Weiche https braucht's nicht. sf_test muss aber umgeschrieben werden.
OK Überlegen: sollte PATCH auch eine x-query-syntax-of-method=GET haben und so als select function aufrufbar sein? Schadet eigentlich nicht, oder?
OK Definiere Default Page
OK Definiere Default Err Page
OK Bring github under control
- Response mit .0 und .1 für Header und Content. Das ist irgendwie Unsinn und geht sicher besser!
OK - Umstellen, so dass mehrere Kontexte gleichzeitig (über einen Port) laufen können
    - Idee: Kontexte in Config mit entsprechenden Unterobjekten anlegen, dann weitersehen. Wie geht das syntaktisch?
    - DEAL WITH //@todo-2021-10-3 in main.rs,