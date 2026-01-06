# Toki - Användarmanual

Toki är ett verktyg för tidrapportering som integrerar med Milltime och Azure DevOps. Denna manual beskriver hur du använder applikationen för att rapportera din tid.

---

## Innehåll

1. [Komma igång](#komma-igång)
2. [Logga in](#logga-in)
3. [Tidrapportering med Milltime](#tidrapportering-med-milltime)
4. [Använda timern](#använda-timern)
5. [Skapa tidspost utan timer](#skapa-tidspost-utan-timer)
6. [Redigera och ta bort tidsposter](#redigera-och-ta-bort-tidsposter)
7. [Visa statistik och sammanfattning](#visa-statistik-och-sammanfattning)
8. [Pull Requests (Azure DevOps)](#pull-requests-azure-devops)
9. [Hantera repositories](#hantera-repositories)
10. [Notifikationer](#notifikationer)
11. [Snabbkommandon](#snabbkommandon)
12. [Vanliga frågor](#vanliga-frågor)

---

## Komma igång

Toki är en webbapplikation som du når via din webbläsare. Applikationen kan även installeras som en app på din dator eller mobil för snabbare åtkomst och push-notifikationer.

### Installera som app (valfritt)

1. Öppna Toki i Chrome, Edge eller Safari
2. Klicka på installationsikonen i adressfältet (eller välj "Installera app" i webbläsarmenyn)
3. Bekräfta installationen

---

## Logga in

### Azure AD-inloggning (huvudinloggning)

1. Gå till Tokis webbadress
2. Klicka på **"Sign in with Azure"**
3. Logga in med ditt företagskonto

### Milltime-inloggning

För att kunna tidrapportera behöver du även logga in på Milltime:

1. Navigera till **Milltime** i sidomenyn (timer-ikonen)
2. Fyll i ditt Milltime-användarnamn och lösenord
3. Klicka på **"Sign in"**

Din Milltime-session sparas i din webbläsare så att du inte behöver logga in varje gång.

---

## Tidrapportering med Milltime

Milltime-sidan är din huvudsakliga arbetsyta för tidrapportering. Här kan du:

- Se alla dina tidsposter
- Starta och stoppa timers
- Skapa, redigera och ta bort tidsposter
- Se statistik över arbetad tid

### Översikt

När du öppnar Milltime-sidan ser du:

- **Veckans statistik** (övre delen): Visar hur många timmar du arbetat, kvarvarande tid och flextid
- **Tidsposter** (huvudområdet): Lista över dina tidrapporter grupperade per dag

---

## Använda timern

Timern är det enklaste sättet att registrera tid. Du startar en timer när du börjar arbeta och sparar den när du är klar.

### Starta en timer

**Via snabbkommando**

1. Tryck `Ctrl+K` (eller `Cmd+K` på Mac)
2. Välj **"Start empty timer"**

**Via timerhistorik**

1. Starta en timer via snabbkommando (`Ctrl+K`)
2. I dialogen som öppnas kan du klicka på klockikonen för att visa historik
3. Dubbelklicka på en tidigare post för att starta en timer med samma inställningar

### Under tiden timern körs

När en timer är aktiv visas en flytande timer-widget i nedre högra hörnet. Denna widget syns på alla sidor i applikationen så länge timern är igång.

- Tiden visas i timer-widgeten
- Du kan lägga till en **anteckning** direkt i fältet
- Klicka på **penna-ikonen** för att ändra projekt/aktivitet
- Minimera widgeten genom att klicka på **minimera-knappen**

### Spara timern

1. Klicka på **spara-knappen** (diskett-ikon) i timer-widgeten
2. Om du inte valt projekt/aktivitet ännu:
   - Välj **Projekt** från listan (sökbar)
   - Välj **Aktivitet** (t.ex. "Systemutveckling")
   - Lägg till en **anteckning** som beskriver arbetet
3. Klicka på **"Save"**

Tidsposten synkas automatiskt till Milltime.

### Avbryta timern

Om du vill ta bort timern utan att spara:

1. Klicka på **papperskorgs-ikonen** i timer-widgeten
2. Bekräfta borttagningen

---

## Skapa tidspost utan timer

Du kan även skapa tidsposter direkt utan att använda timern:

1. Gå till **Milltime**-sidan
2. Klicka på **"New entry"**-knappen
3. Fyll i formuläret:
   - **Projekt**: Välj från listan
   - **Aktivitet**: Välj aktivitetstyp
   - **Anteckning**: Beskriv arbetet
   - **Datum**: Välj dag
   - **Tid**: Ange start- och sluttid ELLER totalt antal timmar
4. Klicka på **"Create"**

---

## Redigera och ta bort tidsposter

### Redigera en tidspost

1. Hitta tidsposten i listan
2. Klicka på **penna-ikonen** till höger om posten
3. Ändra önskade fält:
   - Datum
   - Anteckning
   - Start- och sluttid
   - Totalt antal timmar
4. Klicka utanför formuläret eller tryck Enter för att spara

### Ta bort en tidspost

1. Klicka på **penna-ikonen** för att gå till redigeringsläge
2. Klicka på **papperskorgs-ikonen**
3. Bekräfta borttagningen

### Låsta poster

Tidsposter som tillhör en låst vecka eller månad kan inte redigeras. Dessa visas med en **lås-ikon**.

---

## Visa statistik och sammanfattning

### Veckostatistik

I övre delen av Milltime-sidan visas:

- **Arbetade timmar**: Hur mycket du arbetat denna vecka
- **Kvarvarande timmar**: Hur mycket tid som återstår enligt din arbetstidsplan
- **Flextid**: Din ackumulerade flextid (plus eller minus)

### Datumfiltrering

1. Klicka på **datumväljaren** högst upp
2. Välj ett fördefinierat intervall:
   - Denna vecka
   - Denna månad
   - Detta år
   - Senaste 30 dagarna
   - Senaste 365 dagarna
3. Eller välj ett anpassat datumintervall

### Sammanfattningsvy

När du väljer ett längre datumintervall visas:

- **Totalt antal timmar**
- **Antal projekt**
- **Cirkeldiagram**: Fördelning av tid per projekt
- **Stapeldiagram**: Daglig fördelning av tid

### Slå ihop poster

Aktivera **"Merge entries"** för att kombinera tidsposter med samma projekt, aktivitet och anteckning på samma dag.

---

## Pull Requests (Azure DevOps)

Toki kan visa dina pull requests från Azure DevOps och hjälpa dig att generera tidsrapporteringsnoteringar.

### Visa pull requests

1. Klicka på **PR-ikonen** i sidomenyn
2. Se alla aktiva pull requests från dina följda repositories

### Filtrera pull requests

Använd filtreringsknapparna:

- **My PRs**: Visa endast PRs där du är författare
- **Reviews**: Visa PRs där du är granskare
- **Blocking**: Visa PRs som väntar på din granskning

### PR-detaljer

Klicka på en PR för att se:

- Beskrivning och kommentarer
- Kopplade work items
- Granskningsstatus

### Kopiera tidsrapporteringstext

I PR-detaljvyn finns knappar för att kopiera färdig text för tidsrapportering:

- **Review**: Genererar text för kodgranskning (inkluderar "[CR]")
- **Develop**: Genererar text för utvecklingsarbete

Texten inkluderar work item-nummer och titel i formatet:

```
#123 #456 - [CR] Implementera ny funktion
```

---

## Hantera repositories

För att följa pull requests behöver du lägga till repositories.

### Lägg till ett repository

1. Gå till **Repositories** i sidomenyn
2. Klicka på **"Add repository"**
3. Fyll i:
   - **Organisation**: Azure DevOps-organisationen
   - **Projekt**: Projektnamnet
   - **Repository**: Repository-namnet
   - **PAT-token**: Din Personal Access Token (se nedan)
4. Klicka på **"Add"**

### Skapa PAT-token

1. Gå till Azure DevOps
2. Klicka på ditt profilikon → **Personal access tokens**
3. Klicka på **"New Token"**
4. Ge den ett namn och välj nödvändiga rättigheter (inte helt säker på exakt vilka som används idag)
5. Kopiera tokenen

### Hantera repositories

På repositories-sidan kan du:

- **Följ repository**: Följ ett repository
- **Avfölja repository**: Sluta följa ett repository
- **Notifikationsinställningar**: Konfigurera vilka aviseringar du vill ha för ett repository

---

## Notifikationer

Toki kan skicka notifikationer när saker händer med dina pull requests.

### Notifikationstyper

- **PR stängd**: När en pull request stängs
- **Ny tråd**: När någon startar en ny kommentarstråd
- **Tråd uppdaterad**: När någon svarar på en tråd
- **Omnämnande**: När du blir omnämnd i en kommentar

### Konfigurera notifikationer

**Per repository:**

1. Gå till **Repositories**
2. Klicka på **klocka-ikonen** på repository-kortet
3. Aktivera/avaktivera notifikationstyper för:
   - **Site**: Visas i appen
   - **Push**: Skickas som push-notifikation

**Per PR:**

1. Öppna PR-detaljvyn
2. Klicka på notifikationsikonen
3. Aktivera/avaktivera specifika notifikationstyper

### Visa notifikationer

1. Klicka på **klocka-ikonen** i sidomenyn
2. Se alla olästa notifikationer
3. Klicka på en notifikation för att navigera till relaterad PR
4. Markera som läst genom att klicka på bock-ikonen
5. Använd **"mark all as read"** för att markera alla som lästa
6. Aktivera **"Hide viewed"** för att dölja redan lästa notifikationer

### Aktivera push-notifikationer

För att få push-notifikationer till din enhet (även när Toki inte är öppet) behöver du aktivera detta via notifikationspopupen. Du kan aktivera push-notifikationer på flera enheter, till exempel både din jobbdator och din telefon, om du vill få notiser på båda.

1. Klicka på **klocka-ikonen** i sidomenyn för att öppna notifikationspopupen
2. Klicka på **kugghjuls-ikonen** (Settings) i övre högra hörnet av popupen
3. Klicka på **"Allow browser notifications"** för att ge webbläsaren tillåtelse att skicka notifikationer
4. När webbläsaren frågar, klicka **"Tillåt"** (eller motsvarande på engelska)
5. Klicka på **"Enable Toki notifications"** för att prenumerera på push-notifikationer
6. Ange ett namn för enheten (t.ex. "Jobbdator" eller "Mobil") så att du kan identifiera den senare

**Tips:** Öppna Toki i webbläsaren på din telefon och följ samma steg ovan för att få push-notifikationer direkt till telefonen.

### Hantera prenumererade enheter

I samma inställningsmeny kan du se alla enheter som är prenumererade på push-notifikationer under **"Subscribed devices"**. För att ta bort en enhet:

1. Öppna notifikationsinställningarna (kugghjuls-ikonen)
2. Klicka på enheten du vill ta bort
3. Enheten tas bort och kommer inte längre att få push-notifikationer

---

## Snabbkommandon

Använd `Ctrl+K` (eller `Cmd+K` på Mac) för att öppna snabbkommandomenyn.

| Kommando                 | Beskrivning                      |
| ------------------------ | -------------------------------- |
| **Pull requests**        | Öppna pull requests-sidan        |
| **Milltime**             | Öppna tidrapporteringssidan      |
| **Repositories**         | Öppna repository-hantering       |
| **Start empty timer**    | Starta en timer                  |
| **Save current timer**   | Spara aktiv timer                |
| **Edit current timer**   | Ändra aktiv timers inställningar |
| **Search pull requests** | Sök bland dina pull requests     |
