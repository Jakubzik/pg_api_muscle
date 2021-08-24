/** 
* 2021-06-30 AE
* 
* In submit.js werden Objekte generiert, die an die Datenbank geschickt werden sollen
* oKontext enthält Infos bei neuem Kontext
* oFrage enthält Infos für jede zu speichernde Frage
* (details in beispielobjekte.txt)
*
* Momentan wird das nur per console.log zum Testen angezeigt
* Eigentlich soll es natürlich als POST an die Datenbank
* 
* Du schreibst das sicher in zwei Minuten runter, aber ich möcht's ja lernen und verstehen :-)
* Das ist noch nicht fertig hier, ich weiß nur gerade nicht weiter what I'm doing
* 
**/


/* very basic, nur die Frage */
/* (soll heißen: das reicht so noch nicht) */
/* das wäre dann in JS Zeile 396, innerhalb von final_save() */

fetch('https://etc.as.uni-heidelberg.de:9443/frage', {  // richtige Adresse?
	method: 'POST',
	body: JSON.stringify(oFrage), // wird das so funktionieren?
	headers: {
		'Content-Type': 'application/json'
	}
})
.then(results => results.json())
.then(doweneedthisstep => console.log(doweneedthisstep)); // ist das nur zur überprüfung? kannst du mir den schritt so oder so mal erklären?
.catch(err => console.log(err));


/* this is no use */ 
/* Was hier fehlt ist: Wenn ein _neuer_ Kontext angelegt wird, soll die neue Kontext-ID erst generiert und dann zurückgegeben werden. */
/* also. in dem Fall zuerst: */


if (b_newContext) {

	fetch('https://etc.as.uni-heidelberg.de:9443/frage', {  // richtige Adresse?
		method: 'POST',
		body: JSON.stringify(oKontext), // s. Zeile 376
		headers: {
			'Content-Type': 'application/json' // sonst noch irgendwas?
		}
	})
	.then(... was dann? ); // ?

}