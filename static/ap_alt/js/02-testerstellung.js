
/** 
 * 2021-05-15 (16?)
 * Lektionen dieses Abends:
 * Drag and Drop ist _wirklich_ shit im handling
 * Multi-Select ist jenseits von simplen Anwendungsbereichen sehr eingeschränkt
 * event.target macht bei manchen DOM-Elementen Dinge, die ich überhaupt nicht verstehe
 * müde müde müde 
 * **/


/** 
 * Notes:
 *
 * Ich hab' die Frage-Boxen komplett umgeschrieben,
 * damit das mit div-Elementen statt mit select/option arbeitet.
 * Das Drag and Drop war mit den select options sehr hakelig
 * und funktioniert außerdem in Chrome gar nicht. 
 * Glaube du wirst das mit den divs eh lieber mögen.
 * (Für mich ist das auch besser, die lassen sich besser anpassen.)
 *
 * Aber vielleicht ist der Code da jetzt etwas unschön, let me know if so.
 * (das ist die Funktion clickSelectQuestion )
 *
 * Außerdem sollte das drag and drop jetzt funktionieren? I think? 
 * Also, die Reihenfolge sollte dann direkt beim Drop auch im Array geändert werden.
 * Aber ich sterbe gleich vor Müdigkeit und bin nicht mehr konzentriert,
 * also keine Garantien für irgendwas.
 *
 * (Das lässt sich momentan über den "Speichern"-Button testen, mit der ID-Reihenfolge)
 *
 * Für die Frage-Infos war wegen dieser select-to-div-Sache jetzt doch noch keine Zeit.
 *
 * **/


/** A to H: die Abfragen, die ich benutzt habe, sind unter db-abfragen-ae.txt im gleichen Verzeichnis. Ist alles ganz ganz simpel und nicht überraschend, wollte es nur der Vollständigkeit halber hinterlegen. **/ 


var g_DEBUG_LEVEL = 3; // 0 = silent, 1 = error, 2 = info, 3 = debug



// alle Fragen
var aFragen = []; // verfügbare Fragen im Fragenpool

var aSelectedFragen = []; // für diesen Test ausgewählte Fragen

// nach Kategorien sortierte Fragen (s.u. in getQuestions)
let aSpx = []; 
let aLit = [];
let aLing = [];
let aSonst = [];

// Kategorien, Tags, Fragekontexte (kommen momentan im gleichen fetch)
let aTags;
let aCats;
let aContext;
let aAntworten;

// HTML-Elemente, die wir häufiger brauchen

let eCatSelect = document.getElementById('e_catselect');

let eAllQBox = document.getElementById('all-q-box');

let eCurrQBox = document.getElementById('sel-q-box'); // für den aktuellen Test gewählte Fragen-Box


// Funktion zum Schließen der Frage-Infos wenn man irgendwo im Fenster klickt

window.onclick = function(evt) {
	let frage_info_box = document.getElementById('frage_info_box');
	if (frage_info_box == null) return;
	// wenn irgendwohin außer box selbst oder eine Frage geklickt wird, soll die Box entfernt werden
	// (nicht bei Klick auf eine Frage, weil sonst die Box nie angezeigt wird)
	if ((evt.target.id != 'frage_info_box') && (evt.target.id.indexOf('frage_id') < 0)) { 
		frage_info_box.parentNode.removeChild(frage_info_box);
	}
}

/* JSON holen 2 (testjson für fragen) 

fetch('02-fragen.json').then(
	function(u){ return u.json();}
).then(
	function(data){
		aFragen = data;
		if (g_DEBUG_LEVEL >= 3) console.log('die siebte Frage ist: ' + aFragen[6].frage_text);
	}
)

		if (g_DEBUG_LEVEL >= 3) console.log('number of questions: ' + aFragen.length);*/


/* same result as above, nur anders zum ausprobieren */ 

async function getQuestions() {
	var response = await fetch('02-fragen.json');
	aFragen = await response.json();
	return aFragen;
}

/* holt die Fragenliste, sortiert dann alle Fragen nach Kategorien */ 

getQuestions().then(aFragen => {aFragen;}).then(
		function(){getTagsCats();} // in dieser Reihenfolge funktioniert das, weil dann die Frageliste da und sortiert ist, bevor _irgendwas_ anderes passiert. Solange noch keine Kategorien im Select sind, können auch noch keine ausgewählt werden, deswegen ist das in dieser Kette safe. Aber ist das auch schlau und effizient so..? und warum muss der aufruf von getTagsCats() selbst in eine Funktion gewickelt sein?
);


// sortcats() ist momentan ausgelagert, war vorher innerhalb von getQuestions (direkt vor getTagsCats();). 
// so wie das jetzt ist, werden die Fragen jedes Mal wenn im Dropdown eine Kategorie ausgewählt wird wieder neu in diese Arrays gefiltert.
// (vorher wurden sie beim Holen von aFragen direkt gefiltert)
// Das ist vielleicht zu umständlich. Akut würde es den Vorteil geben, dass man beim Hin- und Herschieben von Fragen nicht erst das richtige Array identifizieren muss, sondern einfach in aFragen pushen oder slicen kann, und dann wird das hinterher sortiert.
// Potenzielle Nachteile -> ? Macht das das Programm zu langsam?


function sortcats() { // sortiert die Fragen in einzelne Arrays nach Kategorie
	if (g_DEBUG_LEVEL >= 3) console.log('[DEBUG:] number of questions: ' + aFragen.length);
	aFragen.sort(function (a, b) {
		return a.frage_id - b.frage_id; // sortiert nach frage-id. wenn Fragen aus der rechten Box gelöscht wurden, werden sie sonst an der falschen Stelle (am Ende der jew. Kategorie) gelistet, was nicht intuitiv ist und ab einer gewissen Masse von Fragen unübersichtlich wird.
	});
	aSpx = aFragen.filter(frage => frage.fragekategorie_id === 1); 
	aLit = aFragen.filter(frage => frage.fragekategorie_id === 2);
	aLing = aFragen.filter(frage => frage.fragekategorie_id === 3);
	aSonst = aFragen.filter(frage => frage.fragekategorie_id === 4);
	if (g_DEBUG_LEVEL >= 3) console.log('[DEBUG:] number of questions in 1: ' + aSpx.length);
}


/* JSON holen (testjson für tags und kategorien) */



function getTagsCats() {
fetch('01-tags-cats.json').then(
	function(u){ return u.json();}
).then(
	function(content){
		aTags = content.fragetags;
		aCats = content.fragekategorie;
		aContext = content.fragekontext;
		aAntworten = content.antwortoption;
		if ((g_DEBUG_LEVEL >= 2) && (aTags.length * aCats.length * aContext.length * aAntworten.length > 0)) console.log('[INFO:] Tags, Kategorien, Kontext, Antwortoptionen erfolgreich geladen.');
		makecatlist();
		maketaglist();
		makecontextlist();
	}
)
}



/* Kategorie-Select befüllen */ 

function makecatlist() {
	aCats.forEach(function(item) {
		if (g_DEBUG_LEVEL >= 3) console.log('[INFO:] found cat nr. ' + item.kategorieid);
		let opt_cat = document.createElement("option");
		opt_cat.value = "kategorie_" + item.kategorieid;
		opt_cat.id = "kategorie_id-" + item.kategorieid;
		opt_cat.innerHTML = item.kategoriename;
		document.getElementById('e_catselect').appendChild(opt_cat);
	});
}


/* Tag-Liste befüllen. Die Tag-Liste ist ein multiple select. Jedes Tag-Element kriegt bei Click eine Funktion, die dann die verfügbaren Fragen filtert. */ 

function maketaglist() {
	aTags.forEach(function(tagitem) {
		let tag_entry = document.createElement("option");
		tag_entry.id = "tag_id-" + tagitem.tagid;
		tag_entry.innerHTML = tagitem.tagname;
		tag_entry.classList.add("taglist-item");
		tag_entry.setAttribute('onclick', 'tagfilter(' + tagitem.tagid + ')');
		document.getElementById('e_tags').appendChild(tag_entry);
	});
}		

/** A to H: Die Funktion, die hierunter steht ("tagfilter") ist momentan nur provisorisch zum prinzipiellen Testen. Wenn dann alles richtig funktioniert, soll das eben die Filterfunktion sein. Das braucht aber deine Hilfe. **/

function tagfilter(xy) {
	if (g_DEBUG_LEVEL >= 3) console.log('[DEBUG:] current tag id: ' + xy);
	let tagList = document.getElementById('e_tags');
	let aSelTags = tagList.selectedOptions;
	if (g_DEBUG_LEVEL >= 3) console.log('[TEST:] number of tags: ' + aSelTags.length); // nur um zu testen, ob selectedOptions funktioniert
}


/* Kontext-Filter-Liste befüllen. Schick wäre, wenn nur die Kontexte angezeigt werden würden, zu denen es in dieser Kategorie überhaupt Fragen gibt, aber das ist gerade zu kompliziert. */

function makecontextlist() {
	aContext.forEach(function(context) {
		let con_entry = document.createElement("div");
		con_entry.id = "context_id-" + context.fragekontext_id;
		con_entry.classList.add("context-item");
		con_entry.setAttribute('onclick', 'contextfilter(' + context.fragekontext_id + ', "context_id-' + context.fragekontext_id+ '")'); // in der funktion, die u.a. die active class zuweist, brauchen wir die kontext_id einzeln _und_ die HTML-Element-ID
		con_entry.innerHTML = context.fragekontext_quelle;
		document.getElementById('e_kontext_sel').appendChild(con_entry);
	});
}

/* Filtert die momentan angezeigten Fragen nach dem gewählten Kontext. Momentan kann immer nur ein Kontext at a time gewählt werden. Es werden dann alle Fragen in der Liste ausgeblendet (also nur optisch, nicht aus dem Array entfernt), und dann nur die wieder eingeblendet, die diesem Kontext zugeordnet sind. */

function contextfilter(conid, elID) {
	if (document.getElementById('all-q-box').innerHTML == '') return;
	// blendet alle Fragen aus (nur visuell, nicht im array)
	let aQuestions = document.getElementsByClassName('q-entry');
	for (let j=0; j<aQuestions.length; j++) {
		aQuestions[j].style.display = "none";
	}
	// blendet die Fragen wieder ein, die dem gewählten Kontext zugeordnet sind
	aFragen.forEach(function(qitem) {
		let qitem_entry = document.getElementById('frage_id-' + qitem.frage_id);
		if (qitem.fragekontext_id == conid && qitem_entry != null) {
			qitem_entry.style.display = "block";
		}
	});
	// gibt dem geklickten Kontext-Element eine "active"-class für visuelles feedback
	removeContextActive();
	document.getElementById(elID).classList.add('context_active');
}


// entfernt die "active"-class vom vorher gewählten Kontext
function removeContextActive() {
	let aContexts = document.getElementsByClassName('context-item');
	for (let jj = 0; jj < aContexts.length; jj++) {
		aContexts[jj].classList.remove('context_active');
	}
}

// Kontext "entwählen": alle Fragen der Kategorie werden wieder angezeigt, active-class entfernt
function clearContext(){
	if (document.getElementById('all-q-box').innerHTML == '') return;
	removeContextActive();
	let aQuestions = document.getElementsByClassName('q-entry');
	for (let j=0; j<aQuestions.length; j++) {
		aQuestions[j].style.display = "block";
	}
}


// wird ausgelöst, wenn man etwas im Kategorie-Dropdown-Menu auswählt

document.getElementById('e_catselect').onchange = function(){
	let eleID = eCatSelect.options[eCatSelect.selectedIndex].id;
	let catID = eleID.substring(eleID.indexOf('-') + 1); // kürzt die element-ID auf den relevanten Teil (die kategorie_id), als str
	sortcats();
	makeQuestionsList(catID);
	removeContextActive();
}



function makeQuestionsList(selid) {
	let eAllQs = document.getElementById('all-q-box');
	if (g_DEBUG_LEVEL >= 3) console.log('[DEBUG:] selected id: ' + selid);
	eAllQs.innerHTML = '';  // leert die Fragen-Box wenn eine neue Kategorie gewählt wird
	let aSelectedCat;
	switch (selid) { // je nachdem, welche Kategorie ausgewählt wurde
		case "1": 
			aSelectedCat = aSpx;
			break;
		case "2": 
			aSelectedCat = aLit;
			break;
		case "3":
			aSelectedCat = aLing;
			break;
		case "4": 
			aSelectedCat = aSonst;
			break;
	}
	aSelectedCat.forEach(function(question) {
		let q_entry = document.createElement("div");
		let e_q_id = "frage_id-" + question.frage_id;
		q_entry.id = e_q_id;
		q_entry.innerHTML = question.frage_text;
		q_entry.classList.add('q-entry');
		q_entry.setAttribute('onclick', 'clickSelectQuestion("' + e_q_id + '", ' + question.frage_id +', "all")');
		document.getElementById('all-q-box').appendChild(q_entry);
	});
	clearContext();
}

// TODO: Antwortoptionen & andere Infos pro Frage anzeigen lassen?


/* Wird durch Klick auf den Pfeil nach rechts (oberer Button) ausgelöst: verschiebt gewählte Fragen in die linke Box (zum aktuellen Testtermin) */ 

function moveQuestionThere() {
	if (eAllQBox.innerHTML == '') return;
	let aSelQs = eAllQBox.getElementsByClassName('clicked_question'); // alle momentan ausgewählten Fragen in der linken Box
	let aSelQIDs = []; // Array mit Frage-IDs der gewählten Fragen
	// gewählte Frage-IDs sammeln
	Array.from(aSelQs).forEach(function(sel_q_item) { 
		let e_sel_qID = sel_q_item.id;
		let sel_qID = e_sel_qID.substring(e_sel_qID.indexOf('-') + 1); // kürzt die element-ID auf den relevanten Teil (die frage_id), als str
		aSelQIDs.push(sel_qID);
	});
	
	
	/** 
	*
	* hier for... statt forEach, weil es rückwärts
	* gehen muss. Sonst gäb es dieses Problem:
	*
	* Wenn man mehrere Fragen auswählt, 
	* die direkt untereinander liegen 
	* (also >2 aufeinanderfolgende Fragen),
	* wird nur jede zweite Frage hinzugefügt. 
	* 
	* Das Problem entsteht im forEach beim aFragen-Array:
	* Wenn eine ID gefunden wurde, wird das Element gelöscht,
	* also rutscht das nächste an die Stelle,
	* und das wird dann nicht mehr beachtet.
	*
	* **/
	
	// gewählte Fragen im aFragen-Array finden
	
	let aTMPSelectedFragen = [];
	for (let j = aFragen.length - 1; j >= 0; j--) {
	let a_q_item = aFragen[j];
		if (aSelQIDs.indexOf(a_q_item.frage_id.toString()) >= 0) { // wenn das aktuelle Frage-Element gewählt ist, d.h. wenn die Frage-ID im aSelQIDs-Array ist
			aTMPSelectedFragen.push(a_q_item); // schiebt die Frage in das "selected" array
			aFragen.splice(aFragen.indexOf(a_q_item), 1); // löscht die Frage aus dem aFragen-Array (aus dem die linke Box befüllt wird)
		}
	}

	// der Schritt über aTMPSelectedFragen löst das Problem, dass sonst mehrere gleichzeitig gewählte Fragen rückwärts übertragen werden würden (wg. j--). So kann immer die aktuelle Auswahl über reverse() umgedreht werden, bevor es im aSelectedFragen-Array landet.
	aTMPSelectedFragen.reverse();	
	aSelectedFragen.push.apply(aSelectedFragen, aTMPSelectedFragen);
	
	// verschobene Fragen ausblenden
	
	let currCat = eCatSelect.options[eCatSelect.selectedIndex].id.substring(eCatSelect.options[eCatSelect.selectedIndex].id.indexOf('-') + 1); // die ID der aktuell gewählten Kategorie
	sortcats(); 
	makeQuestionsList(currCat);
	listSelectedQs();
}



function listSelectedQs() {
	document.getElementById("sel-q-box").innerHTML = "";

	aSelectedFragen.forEach(function(sel_question) { // alle gewählten Fragen sind in dem Array -> in der rechten Box anzeigen
		let sel_q_entry = document.createElement("div");
		let ele_id = "frage_id-" + sel_question.frage_id;
		sel_q_entry.id = ele_id; 
		sel_q_entry.innerHTML = sel_question.frage_text;
		sel_q_entry.classList.add('sel-q-entry');
		sel_q_entry.setAttribute('onclick', 'clickSelectQuestion("' + ele_id + '", ' + sel_question.frage_id +', "sel")');
		sel_q_entry.setAttribute('draggable', 'true');
		document.getElementById('sel-q-box').appendChild(sel_q_entry); 
	});
	let e_endspan = document.createElement("div");
	e_endspan.id = "selq-endspan";
	eCurrQBox.appendChild(e_endspan);

 
/** 
* unexpected snag: 
* in Chrome funktioniert draggable="true" auf option-elements nicht.
*
* mögliche Lösungen:
* 1. Chrome ignorieren
* 2. komplette rechte Box so umschreiben, dass es <div> statt <select> -> <option> sind
*      Problem damit:
*         Dann verhält sich die Auswahl anders als links (mit strg/shift),
*         was unintuitiv wäre. 
*
* let's face it though:
* ich bin mit dem drag and drop handling im multiple select eh sehr unzufrieden.
* 
* seufz
*
* (inzwischen done)
**/
	
	countQuestions(); // zeigt an wie viele Fragen aktuell gewählt sind
}



var b_altKey;
var b_shiftKey;

function altCheck(e) { // prüft, ob alt beim Klick gedrückt war
	b_altKey = e.altKey;
}

function shiftCheck(e) { // prüft, ob Shift beim Klick gedrückt war
	b_shiftKey = e.shiftKey;
}


/* Die Funktion hierunter macht verschiedene Dinge, je nachdem wie geklickt wird. Bei normalem Klick wird das Element an- oder wieder abgewählt (je nach vorigem Zustand). Bei Shift-Klick wird alles zwischen dem letzt-gewählten und diesem Element ausgewählt. Bei ALT-Klick sollen die Frageinfos angezeigt werden (aber nichts ausgewählt). */

var e_lastClickedFrage;

function clickSelectQuestion(eClicked, i_qID, box) {
	// wenn vorher eine Frage-Info-Box geöffnet war, soll sie hier geschlossen werden.
	let e_previous_box = document.getElementById('frage_info_box');
	if (e_previous_box != null) {
		e_previous_box.parentNode.removeChild(e_previous_box);
	}
	// prüfen, ob ALT beim Mausklick gedrückt war
	altCheck(event);
	if (g_DEBUG_LEVEL >= 2) console.log('[INFO:] ALT war ' + (b_altKey ? 'gedrueckt' : 'nicht gedrueckt'));
	// wenn ALT gedrückt war, soll die Frage nicht ausgewählt werden, sondern Frageinfos angezeigt
	if (b_altKey) {
			let e_options = document.createElement('div');
			e_options.id = 'frage_info_box';
			e_options.style.left = event.pageX + 'px';
			e_options.style.top = event.pageY + 'px';
			aAntworten.forEach(function(answer) {
				if(answer.frage_id != i_qID) return; // AE reminder: return in foreach ist äquivalent zu continue in normalen loops.
				let e_answer = document.createElement('p');
				e_answer.classList.add('antwortoption');
				e_answer.textContent = answer.option_id + ": " + answer.option_text;
				if (answer.option_correct) e_answer.classList.add('antwortoption_corr');
				e_options.appendChild(e_answer);
			});
			document.body.appendChild(e_options);
		
		/**
		 *
		 * Inhalt: 
		 * -------
		 * immer Antwortoptionen (mostly done)
		 * wenn vorhanden: zugeordnete Tags
		 * wenn vorhanden: zugeordneter Kontext
		 * Anzahl Tests in denen die Frage bisher verwendet wurde
		 *
		 * TODO:
		 * =====
		 * Überschrift einfügen
		 * Tags, Context, Anzahl Tests: nur anzeigen, wenn nicht null
		 *
		 * **/
		return;
	}
	shiftCheck(event);
	let eClickedQ = document.getElementById(eClicked);
	eClickedQ.classList.toggle('clicked_question');
	// this is not pretty. Ich will, dass bei shiftKey-true alle Fragen zwischen A und B angewählt werden (um das Verhalten vom multiple select zu simulieren). Dafür ist aber wichtig (I think) ob A vor oder nach B liegt. Das wird hier bestimmt, und dann werden die dazwischenliegenden sibling-Elemente (in die eine oder andere Richtung) als geklickt markiert.
	if (b_shiftKey) {
		if (eClicked == e_lastClickedFrage) return;
		let ePrevQ = document.getElementById(e_lastClickedFrage);
		let b_boxCheck;
		if (box == "sel") {
			b_boxCheck = (ePrevQ.parentNode.id == eCurrQBox.id);
		} else if (box == "all") {
			b_boxCheck = (ePrevQ.parentNode.id == eAllQBox.id);
		}
		if (b_boxCheck != true) return; // dann liegt das zuletzt geklickte Element in der anderen Box.
		if (ePrevQ == null) return; // dann wurde vorher kein Element geklickt
		if (ePrevQ.compareDocumentPosition(eClickedQ) & Node.DOCUMENT_POSITION_FOLLOWING) {
			let sibling_a = ePrevQ.nextElementSibling;
			while (sibling_a) {
				if (sibling_a.id == eClickedQ.id) break;
				sibling_a.classList.add('clicked_question');
				sibling_a = sibling_a.nextElementSibling;
			}
		} else if (ePrevQ.compareDocumentPosition(eClickedQ) & Node.DOCUMENT_POSITION_PRECEDING) {
			let sibling_b = eClickedQ.nextElementSibling;
			while (sibling_b) {
				if (sibling_b.id == ePrevQ.id) break;
				sibling_b.classList.add('clicked_question');
				sibling_b = sibling_b.nextElementSibling;
			}
		} else return;
	} 
	if (eClickedQ.classList.contains('clicked_question')) {
		e_lastClickedFrage = eClicked;
	}
}



/** START DRAG AND DROP **/ 

var dragging;

eCurrQBox.addEventListener('dragstart', function(event) {
	dragging = event.target;
	event.dataTransfer.setData('text/html', dragging);
});

eCurrQBox.addEventListener('dragover', function(event) {
	event.preventDefault();
});


// das war ein Versuch für visuelles Feedback. Hat zwar funktioniert, aber kettenweise Fehlermeldungen generiert (event.target.style is undefined). Das Problem besteht nicht, wenn das z.B. li-Elemente sind, oder option-Elemente. Bei div aber schon.

eCurrQBox.addEventListener('dragenter', function(event) {
    if (event.target.id != undefined) event.target.style.margin = '0 0 9px';
   // if (event.target.id != undefined) event.target.style.border = 'none none 3px dashed #f4f1ea';
});

eCurrQBox.addEventListener('dragleave', function(event) {
    if (event.target.id != undefined) event.target.style.margin = '0';
  //  if (event.target.id != undefined) event.target.style.border = 'none';
});


var e_target_sibling;

eCurrQBox.addEventListener('drop', function(event) {
    event.preventDefault();
    if (event.target.id != undefined) event.target.style.margin = '0';
//    if (event.target.id != undefined) event.target.style.border = 'none';
    if (event.target.nextElementSibling.classList.contains('sel-q-entry')) {
	    event.target.parentNode.insertBefore(dragging, event.target.nextSibling);
    } else {
	    eCurrQBox.insertBefore(dragging, document.getElementById('selq-endspan')); // das ist wichtig, damit ein Element nie außerhalb von der Box landet (was sonst passiert, wenn man es nach ganz unten zieht). Der "selq-endspan" wird immer ans Ende angehängt, wenn die selq-liste erzeugt wird.
    }
	e_sibling_id = event.target.id;

	if (g_DEBUG_LEVEL >= 3) console.log('[DEBUG:] dragged id ist ' + dragging.id + ', sib id ist ' + e_sibling_id);
	sortSelectedFragen(dragging.id, e_sibling_id);
});



/** ENDE DRAG AND DROP **/



// modifiziert das aSelectedFragen-Array so, dass die Fragen in der richtigen Reihenfolge gespeichert werden. Mit übergeben werden die ID des verschobenen Elements, und die ID des direkt darauf folgenden HTML Elements. 

function sortSelectedFragen(eMovedID, eNextID) {
	let movedQIndexA; // Ursprungs-Index
	let movedQIndexB; // Ziel-Index
	let endQ;
	let movedQID = eMovedID.substring(eMovedID.indexOf('-') + 1); // die relevante frage_id
	let nextQID;
	if (eNextID.indexOf('frage_id') < 0) { // wenn das nächste Element keine Frage mehr ist ...
		movedQIndexB = aSelectedFragen.length -1; // ... dann soll die Frage ganz ans Ende sortiert werden
		endQ = true;
	} else { 
		nextQID = eNextID.substring(eNextID.indexOf('-') + 1); // die frage_id der nächsten Frage
	}
	aSelectedFragen.forEach(function(frage_item, index) {
		if (frage_item.frage_id.toString() == movedQID) {
			movedQIndexA = index; // wenn die zu verschiebende Frage gefunden wurde: start-index
		}
	});
	if (endQ != true) {
		aSelectedFragen.forEach(function(next_frage_item, next_index) {
			if (next_frage_item.frage_id.toString() == nextQID) {
				if (next_index > movedQIndexA) {
					movedQIndexB = next_index;
				} else if (next_index < movedQIndexA) {  // warum ist das so?
					movedQIndexB = next_index +1; // ziel-index im Array (vor dem folgenden Element)
				}
			}
		});
	}
	if (g_DEBUG_LEVEL >= 3) console.log('[DEBUG:] Index A ist ' + movedQIndexA + ', Index B ist ' + movedQIndexB);
	aSelectedFragen.splice(movedQIndexB, 0, aSelectedFragen.splice(movedQIndexA, 1)[0]); // von hinten: löscht 1 Element beim start-index, und fügt dieses beim ziel-index ein (und löscht dort 0).
}





/* Wird durch Klick auf den Pfeil nach links (unterer Button) ausgelöst: verschiebt gewählte Fragen zurück in den Pool (und löscht sie aus dem aktuellen Testtermin) */

function moveQuestionBack() {
	if (eCurrQBox.innerHTML == "") return;
	let aSelCurrQs = eCurrQBox.getElementsByClassName('clicked_question'); // alle momentan ausgewählten Fragen in der rechten Box (die hierdurch aus dem aktuellen Test entfernt werden sollen)
	let aSelCurrQIDs = [];
	Array.from(aSelCurrQs).forEach(function(sel_cq_item) {
		let e_sel_cqID = sel_cq_item.id;
		let sel_cqID = e_sel_cqID.substring(e_sel_cqID.indexOf('-') + 1); // kürzt die element-ID auf den relevanten Teil (die frage_id), als str
		aSelCurrQIDs.push(sel_cqID);
	});
	if (g_DEBUG_LEVEL >= 3) console.log('[DEBUG:] number of questions to remove: ' + aSelCurrQIDs.length);
	// gewählte Fragen im selected-Fragen-Array des aktuellen Tests finden
	
	for (let i = aSelectedFragen.length - 1; i >= 0; i--) {  // das muss ein for-loop sein, weil ich nicht weiß, wie ein forEach rückwärts geht.
		let a_cq_item = aSelectedFragen[i]
		if (aSelCurrQIDs.indexOf(a_cq_item.frage_id.toString()) >= 0) { // wenn das aktuelle Frage-Element gewählt ist, d.h. wenn die Frage-ID im aSelCurrQIDs-Array ist
			aFragen.push(a_cq_item); // schiebt die Frage in das "alle fragen" array. 
			aSelectedFragen.splice(aSelectedFragen.indexOf(a_cq_item), 1); // löscht die Frage aus dem aSelectedFragen-Array (aus dem die linke Box befüllt wird
		}
	}
	let currCat = eCatSelect.options[eCatSelect.selectedIndex].id.substring(eCatSelect.options[eCatSelect.selectedIndex].id.indexOf('-') + 1); // die ID der aktuell gewählten Kategorie
	sortcats();
	makeQuestionsList(currCat); // lädt die Anzeige links neu
	listSelectedQs();
	countQuestions(); // zeigt an wie viele Fragen aktuell gewählt sind
}



/** 
* hier ist ein Problem:
* Wenn die items in der rechten Box verschoben werden (= geänderte Reihenfolge), müssen sie auch im aSelectedFragen-Array verschoben werden.
* Wie?
**/


/* zählt die Fragen pro Kategorie für die Box ganz rechts */

function countQuestions() {
	let aSelectedSPX = aSelectedFragen.filter(frage => frage.fragekategorie_id === 1); 
	let aSelectedLIT = aSelectedFragen.filter(frage => frage.fragekategorie_id === 2); 
	let aSelectedLING = aSelectedFragen.filter(frage => frage.fragekategorie_id === 3); 
	let aSelectedSONST = aSelectedFragen.filter(frage => frage.fragekategorie_id === 4); 
	document.getElementById("e_fragen_total").innerHTML = aSelectedFragen.length;
	document.getElementById("e_fragen_spx").innerHTML = aSelectedSPX.length;
	document.getElementById("e_fragen_lit").innerHTML = aSelectedLIT.length;
	document.getElementById("e_fragen_ling").innerHTML = aSelectedLING.length;
	document.getElementById("e_fragen_sonst").innerHTML = aSelectedSONST.length;
}


function saveIDs() {
	let aCurrTestQIDs = aSelectedFragen.map(frage => frage.frage_id);
	if (g_DEBUG_LEVEL >= 2) console.log('[INFO:] zu speichernde IDs: ' + aCurrTestQIDs);
}
