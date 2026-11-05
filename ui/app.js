const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const element = (id) => document.getElementById(id);
const times = element("times");
const picker = element("picker");
const levels = {
  country: element("country"),
  city: element("city"),
  district: element("district"),
};
const lists = { country: [], city: [], district: [] };

// A newer selection invalidates any list request still in flight.
let selectionToken = 0;
let hasLocation = false;

function showError(target, message) {
  target.textContent = message ?? "";
  target.hidden = !message;
}

function render(view) {
  hasLocation = view.location !== null;
  element("location").textContent = view.location ?? "Konum seçilmedi";
  element("next").textContent = view.next ?? "";
  element("remaining").textContent = view.remaining ?? "—";

  element("rows").replaceChildren(
    ...view.rows.map((row) => {
      const item = document.createElement("li");
      item.classList.toggle("current", row.current);
      const name = document.createElement("span");
      name.textContent = row.name;
      const time = document.createElement("time");
      time.textContent = row.time;
      item.append(name, time);
      return item;
    }),
  );

  element("zone").textContent = view.timezone_guessed
    ? `Konumun saat dilimi bulunamadı, Mac'in saati kullanılıyor (${view.timezone}).`
    : (view.timezone ?? "");
  showError(element("error"), view.error);
}

function fill(level, items) {
  lists[level] = items;
  levels[level].replaceChildren(
    ...items.map((item) => {
      const option = document.createElement("option");
      option.value = item.id;
      option.textContent = item.name;
      return option;
    }),
  );
}

function chosen(level) {
  return lists[level].find((item) => item.id === levels[level].value);
}

function clear(...names) {
  names.forEach((level) => fill(level, []));
}

function syncSave() {
  element("save").disabled = !(chosen("country") && chosen("city") && chosen("district"));
}

async function load(request, level) {
  const token = ++selectionToken;
  showError(element("picker-error"), null);
  try {
    const items = await request();
    if (token !== selectionToken) return false;
    fill(level, items);
    return true;
  } catch (message) {
    if (token === selectionToken) showError(element("picker-error"), message);
    return false;
  } finally {
    syncSave();
  }
}

async function loadDistricts() {
  clear("district");
  syncSave();
  const city = chosen("city");
  if (city) {
    await load(() => invoke("list_districts", { cityId: city.id }), "district");
  }
}

async function loadCities() {
  clear("city", "district");
  syncSave();
  const country = chosen("country");
  if (country) {
    const loaded = await load(() => invoke("list_cities", { countryId: country.id }), "city");
    if (loaded) await loadDistricts();
  }
}

async function openPicker() {
  times.hidden = true;
  picker.hidden = false;
  element("cancel").hidden = !hasLocation;
  if (lists.country.length === 0) {
    const loaded = await load(() => invoke("list_countries"), "country");
    if (loaded) await loadCities();
  }
  syncSave();
}

function closePicker() {
  picker.hidden = true;
  times.hidden = false;
}

levels.country.addEventListener("change", loadCities);
levels.city.addEventListener("change", loadDistricts);
levels.district.addEventListener("change", syncSave);
element("change").addEventListener("click", openPicker);
element("cancel").addEventListener("click", closePicker);

picker.addEventListener("submit", async (event) => {
  event.preventDefault();
  const save = element("save");
  save.disabled = true;
  save.textContent = "Kaydediliyor…";
  showError(element("picker-error"), null);
  try {
    render(
      await invoke("set_location", {
        selection: {
          country: chosen("country"),
          city: chosen("city"),
          district: chosen("district"),
        },
      }),
    );
    closePicker();
  } catch (message) {
    showError(element("picker-error"), message);
  } finally {
    save.textContent = "Kaydet";
    syncSave();
  }
});

listen("view-changed", (event) => render(event.payload));

invoke("get_view").then((view) => {
  render(view);
  if (!hasLocation) openPicker();
});
