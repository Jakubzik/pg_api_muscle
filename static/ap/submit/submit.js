/**
* JS für Aufnahmetest-Frage-Vorschlag (2021)
**/


/**
* TODO
* ====
* - ein paar Sachen aufhübschen
* - Intro-Text schreiben
* - DATENBANKKONTAKT!! Sowohl um Sachen zu holen (Kategorien, Tags, Kontexte) als auch um Fragen, Antwortoptionen und Kontexte etc. zu speichern
* 		-> Momentan ist das noch über JSON
* - Testen, testen
**/

/* vielleicht TODO: window.onload resetAllFields() -> alle Felder leeren, damit bei F5 auch alles leer ist */


var g_DEBUG_LEVEL = 3;	// 0=silent, 1=err, 2=info, 3 = debug


// Kategorien, Tags, Fragekontexte (kommen momentan im gleichen fetch)
let aTags;
let aCats;
let aContext;
let aAufgabentext;

// HTML-Elemente, die wir häufiger brauchen

let eCatSelect = document.getElementById('e_catSelect');
let eTaskSelect = document.getElementById('e_taskSelect');
let eTaskText = document.getElementById('e_taskText');
let eTaskExample = document.getElementById('e_taskExample');
let eContextCheck = document.getElementById('e_contextcheck');
let eContextSelect = document.getElementById('e_contextSelect');
let eContextBox = document.getElementById('e_textExcerpt');
let eContextSource = document.getElementById('e_textSource');
let eTagSelectBox = document.getElementById('e_tagSelection');

let aRadioOpts = document.getElementsByClassName('answer_radio');


// Funktional-Variablen

let b_contextQ = false; // wenn "speichern"/"submit" geklickt wird soll geprüft werden, ob das eine Frage mit Kontext ist (dann: "weitere Frage eingeben" Box) oder nicht.
let b_newContext = false; // um beim Klick auf "speichern"/"submit" an die DB weiterzureichen, ob ein neuer Kontext angelegt werden soll.

/**
* altes FETCH aus statischem JSON; soll jetzt durch Datenbankkommunikation abgelöst werden.

fetch('tags-cats-context.json').then(
	function(u){ return u.json();}
).then(
	function(content){
		aTags = content.fragetags;
		aCats = content.fragekategorie;
		aContext = content.fragekontext;
		if ((g_DEBUG_LEVEL >= 2) && (aTags.length * aCats.length * aContext.length > 0)) console.log('[INFO:] Tags, Kategorien, Kontext, erfolgreich geladen.');
		makecatlist();
		maketaglist();
		makecontextlist();
	}
)

*
**/

/* Tags, Kategorien, Kontexte aus der Datenbank holen */ 

let tkk_url = "https://" + window.location.hostname + ":" + window.location.port + "/tkk";

fetch (tkk_url, {
	method: 'GET',
	headers: {
			'Accept': 'application/json',
			'Content-Type': 'application/json',
			'Prefer': 'return=representation'
	}
}).then(
	function(tkk_data){return tkk_data.json();}
).then(
	function(content){
		aTags = content[0].t_k_k.fragetags;
		aCats = content[0].t_k_k.fragekategorie;
		aContext = content[0].t_k_k.fragekontext;
		aAufgabentext = content[0].t_k_k.aufgabenstellung;
		if ((g_DEBUG_LEVEL) >= 2 && (aTags.length * aCats.length * aContext.length > 0)) console.log('[INFO:] Tags, Kategorien, Kontexte erfolgreich geladen.');
		makecatlist();
		maketaglist();
		makecontextlist();
		maketasklist();
	}
)




/* Das Kategorie-Select soll aus der Datenbank befüllt werden */ 

function makecatlist() {
	aCats.forEach(function(cat) {
		let opt_cat = document.createElement("option");
		opt_cat.value = "kategorie_" + cat.kategorieid;
		opt_cat.id = "kategorie_id-" + cat.kategorieid;
		opt_cat.innerHTML = cat.kategoriename;
		eCatSelect.appendChild(opt_cat);
	});
}




/* Neben der Kategorie-Auswahl ist eine Checkbox für Frage-Kontext ja/nein. Falls ja: Kontextauswahl anzeigen. */

function maketaglist() {
	aTags.forEach(function(tagitem) {
		let tag_entry = document.createElement("option");
		tag_entry.id = "tag_id-" + tagitem.tagid;
		tag_entry.innerHTML = tagitem.tagname;
		tag_entry.classList.add("taglist-item");
		document.getElementById('e_tagSelection').appendChild(tag_entry);
	});
}


function makecontextlist() {
	aContext.forEach(function(context) {
		let opt_txt = document.createElement("option");
		opt_txt.value = "kontext_" + context.fragekontext_id;
		opt_txt.id = "fragekontext_id-" + context.fragekontext_id;
		opt_txt.innerHTML = context.fragekontext_quelle;
		eContextSelect.appendChild(opt_txt);
	});
}

function maketasklist() {
	aAufgabentext.forEach(function(aufgabe) {
		let opt_task = document.createElement('option');
		opt_task.value= "aufgabenstellung_" + aufgabe.aufgabenstellung_id;
		opt_task.id = "aufgabenstellung_id-" + aufgabe.aufgabenstellung_id;
		opt_task.innerHTML = aufgabe.aufgabenstellung_kurzbezeichnung;
		eTaskSelect.appendChild(opt_task);
	});
}



/* Wenn eine Aufgabenstellung ausgewählt wird (oder "keine Aufgabenstellung"), soll der entsprechende Aufgabentext und ggfs. ein zugehöriges Beispiel angezeigt werden (in zwei separaten Boxen?). Wenn es kein Beispiel gibt, soll nur der Aufgabentext angezeigt werden, keine leere Bsp.-Box. */

eTaskSelect.onchange = function(){
	let selectedTask = eTaskSelect.options[eTaskSelect.selectedIndex].id;
	if (selectedTask == "task_id-0") {
		eTaskText.innerHTML = "Zusätzlich zum Fragetext kann im Aufnahmetest eine Aufgabenstellung zu einer Frage oder einer Gruppe von Fragen angezeigt werden.";
		eTaskExample.innerHTML = "";
		eTaskExample.style.display = 'none';
	} else {
		let selTaskID = selectedTask.substring(selectedTask.indexOf('-') + 1); // kürzt die ID vom HTML-Element auf die aufgabenstellung_id
		aAufgabentext.forEach(function(aufgaben_item) {
			if (aufgaben_item.aufgabenstellung_id.toString() == selTaskID) {
				eTaskText.innerHTML = aufgaben_item.aufgabenstellung_text;
				if (aufgaben_item.aufgabenstellung_beispiel != "") { // wenn es zu der Aufgabenstellung ein Beispiel gibt
					eTaskExample.innerHTML = aufgaben_item.aufgabenstellung_beispiel;
					eTaskExample.style.display = 'block';
				} else {
					eTaskExample.innerHTML = ""; // wenn es zu der Aufgabenstellung kein Beispiel gibt soll die Beispielbox leer und auch ausgeblendet sein
					eTaskExample.style.display = 'none';
				}
			}
		});
	}
}

/**
 *
 * let eTaskSelect = document.getElementById('e_taskSelect');
 * let eTaskText = document.getElementById('e_taskText');
 * **/

/** TODO
* Es gibt:
*   aufgabenstellung_text -> wird in einem div statisch angezeigt (nach auswahl) -> erst später
*   aufgabenstellung_beispiel -> wird (falls vorhanden) in einem div angezeigt -> späte
* onchange: text usw. anzeigen, gewaehlte option loggen beim speichernr
**/


/* Wenn die "Fragekontext?"-Checkbox geklickt wird -> prüfen, ob ja/nein. Falls ja: Auswahlliste für Kontexte einblenden (neuer Kontext oder bestehenden Text auswählen). Falls nein: Kontext-Select zurücksetzen (damit die untere Box auch geleert wird) und ausblenden; Text- und Source-Box ausblenden; Kontext-Select ausblenden. */

eContextCheck.onchange = function(){
	if (eContextCheck.checked == true) {
		eContextSelect.style.display = "block";
		b_contextQ = true;
	} else {
		eContextSelect.style.display = "none";
		document.getElementById('kontextbox').style.display = 'none';
		eContextSelect.selectedIndex = null;
		b_contextQ = false;
	}
}

/* Bei Auswahl im Kontext-Select: schauen, ob neuer oder bestehender Text */ 

eContextSelect.onchange = function(){
	let selContext = eContextSelect.options[eContextSelect.selectedIndex].id;
	if (selContext == 'new_context') newcontext();
	else existingcontext(selContext);
}


/* wenn eine neue Frage für einen neuen Kontext eingegeben werden soll */ 

function newcontext() {
	eContextBox.value = '';
	eContextSource.value = '';
	eContextBox.disabled = false;
	eContextSource.disabled = false;
	b_newContext = true; // um später festzulegen, ob ein neuer Kontext an die DB geschickt werden soll
	document.getElementById('kontextbox').style.display = 'block';
}


/* wenn eine neue Frage für einen bestehenden Kontext eingegeben werden soll: befüllt die Kontext-Box und Source-Box und sperrt sie, damit der Text nicht bearbeitet werden kann. (Wenn eine neue Frage für einen bestehenden Text angelegt wird, soll der nicht verändert werden, da in der Datenbank ja andere Fragen damit verknüpft sind.) */

// momentan wird das direkt als string übertragen, mit <br> tags usw. Soll das noch visuell sauber formatiert werden? A TO H: Wie das generell funktioniert, also z.B. Zeilenumbruch-Formatierung oder sogar kursivierung zu übertragen, ggfs. Sonderzeichen und so -- not sure; help? 

function existingcontext(selectedOpt) {
	aContext.forEach(function(context_item) {
		let selContextID = selectedOpt.substring(selectedOpt.indexOf('-') + 1); // kürzt die ID vom HTML-Element auf die Kontext-ID
		if (context_item.fragekontext_id.toString() == selContextID) {
			eContextBox.value = context_item.fragekontext_text;
			eContextSource.value = context_item.fragekontext_quelle;
			eContextBox.disabled = true;
			eContextSource.disabled = true;
			b_newContext = false; // um später festzulegen, ob ein neuer Kontext an die DB geschickt werden soll
		}
	});
	document.getElementById('kontextbox').style.display = 'block';

}


let b_reqFields = false;

let aQuestionData = [];
let aAnswersData = [];
let aTagsData = [];
let aContextData = [];


function submitQuestion() {
	checkRequired();
	if((g_DEBUG_LEVEL >= 3) && (b_reqFields)) console.log('alle Pflichtfelder ok');
	if (b_reqFields) showSaveCheck();
}


/* 
	alert('Vielen Dank, Ihr Fragevorschlag wurde gespeichert.'); */

// Pflichtfelder prüfen

/* 
 * TODO: 
 * 1. Die alert-messages könnten noch ein bisschen höflicher formuliert sein.
 * 2. Bei den Input-Feldern (Frage, 4xAntwort, Kontext, Quelle): da wollen wir sicher eine max-length. Was soll das sein, und prüfen wir das auch in diesem Schritt?
 *
 * */

function checkRequired() {
	b_reqFields = false;	
	if (eCatSelect.options[eCatSelect.selectedIndex].id.length <= 0) {
		alert('Kategorie fehlt');
		return;
	}
	if ((eContextCheck.checked) && (eContextSelect.options[eContextSelect.selectedIndex].id.length <= 0)) {
		alert(unescape('Kontext ausw%E4hlen'));
		return;
	}
	if ((eContextSelect.options[eContextSelect.selectedIndex].id.length > 0) && (eContextBox.value.length * eContextSource.value.length <= 0)) {  // Kontext und Quelle müssen beide da sein
		alert('Kontext und Quelle eingeben');
		return;
	}
	if (document.getElementById('questionText').value.length <= 0) {
		alert('Frage eingeben');
		return;
	}
	if (document.getElementById('answerA').value.trim().length * document.getElementById('answerB').value.trim().length * document.getElementById('answerC').value.trim().length * document.getElementById('answerD').value.trim().length <= 0) { 
		alert('vier Antwortoptionen eingeben');
		return;
	}

	// prüft, ob ein radio button ausgewählt ist
	//
		let aRadioTest = document.getElementsByClassName('answer_radio');
		let correctanswer;
		Array.from(aRadioTest).forEach(function(radio_test_item) {
			if (radio_test_item.checked) {
				correctanswer = radio_test_item.value;
			}
		});
	if (correctanswer == undefined) {
		alert('eine Antwort als richtig markieren');
		return;
	}

	b_reqFields = true;
}



// sammelt die eingegebenen Daten und baut ein Array

/**
 *
 * 2021-05-29
 * NEUER PLAN
 * ==========
 *
 * Objekt so konstruieren:
 *
  frage:{
    fragekategorie_id: 2,
    fragekontext_id: 1,
    frage_text: "She _____ you",
    frage_tags:[3,5,7,11],
    antwortoptionen: [{
     	 option_id: "A",
     	 text: "misses"
        },
	{
	 option_id: "B",
	 text: "loves"
	},
	{
	 option_id: "C",
	 text: "needs"
	}, 
	{
	 option_id: "D",
	 text: "all of the above",
	 option_correct: true
	}]
  }
 *
 * Dann Einträge anpassen statt für jedes ein eigenes Objekt zu bauen
 *
 * FRAGE (A TO H): muss das eigentlich ein Array sein, oder ginge auch:
 *
   oFrage = {fragekategorie_id: 2, fragekontext_id: 1, ...} 
 *
 * weil wir ja ohnehin immer nur eine Frage at a time schicken?
 *
 * (testweise jetzt mal so)
 *
 * **/

// nach dem Speichern: dialogbox einblenden

let e_overlayBox = document.getElementById('save_dialogBackground'); // graut den Hintergrund hinter der dialog-box aus
let e_saveDialog = document.getElementById('save_dialog'); // dialog-box 
let e_saveContextCheck = document.getElementById('save_pre_check'); // zeigt Frage und Antwortoptionen an
let e_saveContextY = document.getElementById('save_w_context'); // falls Kontextfrage: weitere Frage zu diesem Kontext anlegen-Abfrage
let e_saveContextN = document.getElementById('save_no_context'); // falls keine Kontextfrage: nur speichern ok meldung


function showSaveCheck(){
	// spans mit input befüllen
	document.getElementById('savecheck_qtext').textContent = document.getElementById('questionText').value;
	document.getElementById('savecheck_aA').textContent = document.getElementById('answerA').value;
	document.getElementById('savecheck_aB').textContent = document.getElementById('answerB').value;
	document.getElementById('savecheck_aC').textContent = document.getElementById('answerC').value;
	document.getElementById('savecheck_aD').textContent = document.getElementById('answerD').value;
	
		let check_opt_correct;
		Array.from(aRadioOpts).forEach(function(radio_test_item) {
			if (radio_test_item.checked) {
				check_opt_correct = radio_test_item.value;
			}
		});
	document.getElementById('savecheck_corr').textContent = check_opt_correct;	
	
	e_overlayBox.style.display = 'block';
	e_saveContextCheck.style.display = 'block';
	e_saveDialog.style.display = 'block';
}

function keep_editing(){
	e_overlayBox.style.display = 'none';
	e_saveContextY.style.display = 'none';
	e_saveContextN.style.display = 'none';
	e_saveContextCheck.style.display = 'none';
	e_saveDialog.style.display = 'none';
}

let iContextID;  // die Kontext-ID falls es eine Kontext-Frage ist (bestehende ID, oder dann neu angelegte). Soll auch außerhalb dieser Funktion nutzbar sein, für das Neuladen von Fragekontexten.
let iCatID;
let oFrage;

function final_save(){  // zur Übersichtlichkeit in einzelne Funktionen aufteilen?
	
	if (!b_reqFields) {
		alert("Felder fehlen");
		return;
	}
	// Kategorie
	let selectedCat = eCatSelect.options[eCatSelect.selectedIndex].id;
	iCatID = parseInt(selectedCat.substring(selectedCat.indexOf('-') + 1), 10);
	
	// Fragetext
	let sFrageText = document.getElementById('questionText').value;
	
	// OBJEKT ANLEGEN 
	oFrage = {
			fragekategorie_id: iCatID,
			fragekontext_id: "nichts", // ist hier ein i (oder minuszahl) besser als ein str? egal?
			frage_text: sFrageText,
			antwortoptionen: [],
			frage_tags: []
		};
	
	// Antworten
	let aAnswerElements = document.getElementsByClassName('a_textbox');
	// Antworten holen, ID (Buchstabe A-D) abschneiden, prüfen ob der zugehörige Radio Button checked ist ( = richtige Antwort).
	Array.from(aAnswerElements).forEach(function(answer){
		let antwortID = answer.id.slice(-1); // der letzte Buchstabe der HTML-Element-ID
		let radio_bool = false; // wird im nächsten Schritt bearbeitet 
		let aAllRadios = document.getElementsByClassName('answer_radio');
		Array.from(aAllRadios).forEach(function(radio_item){
			if (radio_item.value == antwortID) { // radio values sind im HTML A-D
				radio_bool = radio_item.checked;
			}
		});

	// pro Antwort (von 4): option_id, option_text, option_correct einsammeln
		oFrage.antwortoptionen.push( // A TO H: geht das so?
			{
				option_id: antwortID,  // A, B, C, D
				option_text: answer.value, // der eingegebene Antworttext
				option_correct: radio_bool // true/false (momentan: nur eine Antwort pro Frage true)
		});
	});

	if (g_DEBUG_LEVEL >= 3) console.log('[DEBUG:] geloggte Antwort (' + oFrage.antwortoptionen[2].option_id + ') ist ' + (oFrage.antwortoptionen[2].option_correct ? 'richtig' : 'falsch'));
	if (g_DEBUG_LEVEL >= 3) console.log('[DEBUG:] es gibt ' + oFrage.antwortoptionen.length + ' Antworten.');
	
	// Tags
	
	if (eTagSelectBox.selectedOptions.length > 0) { // (nur, wenn überhaupt Tags gewählt sind) für t_frage_x_tag: fragetag_id
		let aTagSel = eTagSelectBox.selectedOptions;
		Array.from(aTagSel).forEach(function(tagelement){
			let tagID = parseInt(tagelement.id.substring(tagelement.id.indexOf('-') + 1), 10);
			oFrage.frage_tags.push(tagID); 
		});
	}

	// Kontext
	/* A TO H: müssen wir noch besprechen; ggfs. auslagern und umsortieren */
	if (b_contextQ) { // falls es eine Kontextfrage ist
		if (b_newContext) { // d.h. neuer Kontext soll angelegt werden 
			// -> Textauszug Inhalt und Quellenangabe sammeln
			let sConText = eContextBox.value;
			let sConSource = eContextSource.value;
			let oKontext = {
				fragekontext_text: sConText,
				fragekontext_quelle: sConSource
			}
			if (g_DEBUG_LEVEL >= 3) console.log('[DEBUG:] neuer Text erfasst');
			if (g_DEBUG_LEVEL >= 2) console.log('[INFO:] das Kontext-Objekt ist: ');
			if (g_DEBUG_LEVEL >= 2) console.log(oKontext);
			/* A TO H: Das Kontext-Objekt an die DB schicken, als neuen Kontext anlegen, kontext-ID zurückliefern. */

			/* TEST */

		let kontext_url = "https://" + window.location.hostname + ":" + window.location.port + "/fragekontext";

		fetch (kontext_url, {
     		  	method: 'post', 
       	 		headers: {
               		 'Accept': 'application/json',
              		  'Content-Type': 'applicatoin/json', 
              		  'Prefer': 'return=representation'
              		},
              		body:JSON.stringify( oKontext )
		}).then(function(response) {
        	        if (response.status !== 200) {
             	 	        alert('Kontext konnte nicht gespeichert werden');
                	        return;
             	 	}

        	        response.json().then(function(condata) {
            	        	if (g_DEBUG_LEVEL >= 2) console.log('[INFO:] Zurückgegebener Kontext ist: ' + condata.fragekontext_id);
				iContextID = condata.fragekontext_id;
				frageobjekt_senden(iContextID);
	                });
		}).catch(function(err) {
	       		 alert(" err ");
	       		 console.log(err);
		});

			/* TEST ENDE */

		} else { // wenn KEIN neuer Kontext angelegt, sondern ein bestehender genutzt wird
			let selContext = eContextSelect.options[eContextSelect.selectedIndex].id;
			let selContextID = selContext.substring(selContext.indexOf('-') +1);
			iContextID = parseInt(selContextID, 10);
			console.log(iContextID);
			frageobjekt_senden(iContextID);
		}
	} else { // für Fragen ohne Kontext
		frageobjekt_senden("nichts");
	}
 	

}

function frageobjekt_senden(kontextid) {
	if (g_DEBUG_LEVEL >= 3) console.log('[DEBUG:] ' + kontextid);
	oFrage.fragekontext_id = kontextid; 
	
	
	/* Jetzt müsste das oFrage-Objekt komplett sein. */
	if (g_DEBUG_LEVEL >= 2) console.log('[INFO:] das Objekt ist: ');
	if (g_DEBUG_LEVEL >= 2) console.log(oFrage);


	let url = "https://" + window.location.hostname + ":" + window.location.port + "/frage";

	oFrage = { "frage": oFrage };
	fetch(url, {
		method: 'post',
		headers: {
			'Accept': 'application/json',
			'Content-Type': 'application/json',
			'Prefer': 'return=representation'
		},
		body:JSON.stringify( oFrage )
	}).then(function(response) {
		if (response.status !== 200) {
			alert("Speichern fehlgeschlagen");
			return;
		}

		response.json().then(function(data) {
			showSaveDialog();
			console.log( data );
		});
	}).catch(function(err) {
		alert(" err ");
		console.log(err);
	});
	
	
}




/** 
* 
* Bei Klick auf den Submit-Button:
* ================================
*
* Prüfen, ob...
*   - Kategorie ausgewählt
*   - Frage eingegeben
*   - alle vier Antwortoptionen eingegeben
*   - falls b_contextQ = true (also: fragekontext-box is checked): Frage & Source eingegeben?
*   - eine Frage als richtig markiert
*   - (tags optional, I think)
* [=> done]
*
* An die Datenbank weitergeben: 
*   - kategorie_id, frage, vier antwortoptionen (jeweils mit A B C D und correct t/f)
*   - optional (falls gewählt) -> tags (also: tag_ids)
* [=> done]
*   - falls fragekontext y und neuer text -> textauszug und quellenangabe
*   - falls fragekontext y und existing text -> fragekontext_id 
* [=> TODO]
*
* Zur Nutzerführung: [=> TODO]
*   - Erfolgsmeldung anzeigen? ("vielen Dank für Ihre Einreichung, Ihre Frage wurde gespeichert"?)
*   - falls b_contextQ = true -> Meldung: weitere Frage zu diesem Text eingeben? 
*   	- falls ja: vorigen Kontext (egal ob der neu oder bestehend war) und Quellenangabe anzeigen, aber sperren (context.disabled = true), damit es nicht geändert werden kann. b_newContext = false. Frage und Antwort-Felder löschen, radio buttons leeren, tags leeren. Kategorie-Select und Kontext-Select behalten, Kontext-Checkbox.checked. 
* 	- falls nein: alle Felder zurücksetzen? 
*
* **/




function showSaveDialog() {
	// NUR wenn erfolgreich gespeichert wurde (gibt es eine Erfolgsmeldung von der DB?)
	e_overlayBox.style.display = 'block';
	e_saveContextCheck.style.display = 'none'; // save-check wieder ausblenden
	let e_saveContent = b_contextQ ? e_saveContextY : e_saveContextN;
	e_saveContent.style.display = 'block';
	e_saveDialog.style.display = 'block';
}


function reloadContext() {
	resetAllFields(); // leert Frage-Input, Antworten-Input, Radio-Buttons

	if (!b_newContext) { 
		// wenn der gewählte Kontext vorher schon da war
		// -> 2021-06-07: idealerweise irrelevant, wenn die kontext-id aus der db zurückkommt

	}

	
	/**
	 * 
	 * Wenn eine weitere Frage zum selben Kontext gespeichert wird,
	 * soll der gleiche Kontext wie vorher stehen bleiben.
	 *
	 * Das heißt also:
	 * ---------------
	 *  Wenn ein neuer Text abgespeichert wird, müsste die Datenbank
	 *  nach dem Speichern die ID von diesem neu angelegten Text zurückgeben.
	 *  Dann wird das Kontext-Select neu geladen
	 *  und der neu angelegte Text ausgewählt 
	 *  und alle anderen Felder geleert. 
	 *  (das mach ich alles im JS, brauche aber eben die neue ID aus der Datenbank)
	 *
	 * **/
// iCatID
	// vorher gewählte Kategorie wieder klicken
	let cat_value = "kategorie_" + iCatID;
	eCatSelect.value = cat_value;

	// context true check außerdem setzen
	eContextCheck.click();
// den vorher gewählten (oder neu angelegten) Kontext wieder auswählen (per iContextID)
	
	let previous_context_id = "fragekontext_id-" + iContextID;
	let previous_context_value = "kontext_" + iContextID;
	eContextSelect.value = previous_context_value;
	existingcontext(previous_context_id);

// value ist zB "kontext_3"

	closeSaveDialog(); // schließt das overlay und die Dialogbox-Abfrage
}



function allNewQ() {
	resetAllFields();
	closeSaveDialog();
}


// Input-Boxen für Frage und 4x Antwort leeren, Radio-Buttons resetten
// das braucht jede der post-save-Funktionen (regardless of context)
// actually: alles leeren, das ist sauberer.
function resetAllFields() {
	// frage input box
	document.getElementById('questionText').value = "";
	// alle antwort boxen 
	let aAnswerFields = document.getElementsByClassName('a_textbox');
	Array.from(aAnswerFields).forEach(function(answerbox) {
		answerbox.value = "";
	});
	// radio button 
	Array.from(aRadioOpts).forEach(function(radio_test_item) {
			if (radio_test_item.checked) {
				radio_test_item.checked = false;
			}
		});
	// tags, falls gewählt? oder die lassen? -> lieber löschen TODO
	// Kontext
	eContextBox.value = "";
	eContextBox.disabled = false;
	eContextSource.value = "";
	eContextSource.disabled = false;
	// kontext true false checkbox uncheck
	eContextCheck.checked = false;
	// kontext select
	eContextSelect.selectedIndex = null;
	// kategorie select
	eCatSelect.selectedIndex = null;
	// bools schauen -- context q, new context, req fields? (redundant) TODO
	b_contextQ = false; // falls eine neue Frage zum selben Kontext eingegeben werden soll, wird das von selbst wieder aktiviert
	b_newContext = false;
	document.getElementById('kontextbox').style.display = 'none'; // context_text und source ausblenden; wird bei Folgefrage eh wieder eingeblendet.
	eContextSelect.style.display = 'none';
}

function closeSaveDialog() {
	e_saveContextY.style.display = 'none';
	e_saveContextN.style.display = 'none';
	e_saveDialog.style.display = 'none';
	e_overlayBox.style.display = 'none';
}
