package customskins;

import com.google.gson.*;
import java.io.*;
import java.net.URI;
import java.net.http.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.time.Duration;
import java.util.*;

final class SkinData {
    record Skin(String value, String signature) {}
    record Choice(String username, Skin skin, String moderated) {}
    record Cached(Skin skin, long expires) {}
    private final Path file;
    private final Map<String,Choice> choices = new TreeMap<>();
    private final Map<String,Cached> cache = new LinkedHashMap<>();
    private final Map<String,Skin> defaults = new HashMap<>();
    private static final Gson JSON = new GsonBuilder().setPrettyPrinting().create();
    private final HttpClient http = HttpClient.newBuilder().connectTimeout(Duration.ofSeconds(3))
        .followRedirects(HttpClient.Redirect.NEVER).build();

    SkinData(Path directory) throws IOException {
        Files.createDirectories(directory);
        file=directory.resolve("skins.json");
        try(InputStream input=SkinData.class.getResourceAsStream("/defaults.json")) {
            if(input==null)throw new IOException("Missing default skin resource");
            JsonObject root=JsonParser.parseString(new String(input.readAllBytes(),StandardCharsets.UTF_8)).getAsJsonObject();
            for(String key:List.of("steve","alex")) defaults.put(key,parseSkin(root.getAsJsonObject(key)));
        }
        if(Files.exists(file)) {
            if(Files.size(file)>16*1024*1024)throw new IOException("Skin store is too large");
            try {
                JsonObject root=JsonParser.parseString(Files.readString(file)).getAsJsonObject();
                for(var entry:root.entrySet()) {
                    String key=name(entry.getKey());
                    Choice value=JSON.fromJson(entry.getValue(),Choice.class);
                    if(value==null||value.skin()==null)throw new IllegalArgumentException("Missing skin");
                    validateSkin(value.skin());
                    if(value.moderated()!=null&&!List.of("steve","alex").contains(value.moderated()))throw new IllegalArgumentException("Invalid default");
                    if(value.username()!=null)name(value.username());
                    choices.put(key,value);
                }
            } catch(RuntimeException error) {throw new IOException("Invalid skins.json; existing data was left untouched",error);}
        }
    }
    static String name(String value) {
        if(value==null||!value.matches("[A-Za-z0-9_]{1,16}"))
            throw new IllegalArgumentException("Use a Minecraft username (1–16 letters, numbers or underscores). Image URLs are not enabled.");
        return value.toLowerCase(Locale.ROOT);
    }
    static Skin parseSkin(JsonObject property) {
        if(property==null)throw new IllegalArgumentException("Missing textures");
        Skin skin=new Skin(property.get("value").getAsString(),property.get("signature").getAsString());
        validateSkin(skin);return skin;
    }
    static void validateSkin(Skin skin) {
        if(skin==null||skin.value()==null||skin.signature()==null||skin.value().length()>16384||skin.signature().length()>4096||skin.signature().isEmpty())throw new IllegalArgumentException("Invalid signed texture");
        JsonObject body=JsonParser.parseString(new String(Base64.getDecoder().decode(skin.value()),StandardCharsets.UTF_8)).getAsJsonObject();
        URI uri=URI.create(body.getAsJsonObject("textures").getAsJsonObject("SKIN").get("url").getAsString());
        if(!List.of("http","https").contains(uri.getScheme())||!"textures.minecraft.net".equals(uri.getHost())||uri.getUserInfo()!=null||uri.getPort()!=-1||!uri.getPath().matches("/texture/[a-f0-9]{32,64}")||uri.getQuery()!=null||uri.getFragment()!=null)
            throw new IllegalArgumentException("The official API returned an invalid texture URL");
    }
    synchronized Choice choice(String player) {return choices.get(name(player));}
    synchronized Skin cached(String player) {Cached c=cache.get(name(player));return c==null?null:c.skin();}
    Skin defaultSkin(String model) {Skin skin=defaults.get(model);if(skin==null)throw new IllegalArgumentException("Choose steve or alex");return skin;}
    synchronized void put(String player, Choice value) throws IOException {
        String key=name(player);
        if(!choices.containsKey(key)&&choices.size()>=4096)throw new IOException("Skin storage limit reached");
        TreeMap<String,Choice> next=new TreeMap<>(choices);
        if(value==null)next.remove(key);else next.put(key,value);
        Path temp=file.resolveSibling("skins.json.tmp");
        Files.writeString(temp,JSON.toJson(next),StandardOpenOption.CREATE,StandardOpenOption.TRUNCATE_EXISTING);
        // Commit the persisted map before publishing it to login/event threads.
        try {Files.move(temp,file,StandardCopyOption.REPLACE_EXISTING,StandardCopyOption.ATOMIC_MOVE);}
        catch(AtomicMoveNotSupportedException e){Files.move(temp,file,StandardCopyOption.REPLACE_EXISTING);}
        choices.clear();choices.putAll(next);
    }
    Skin official(String username) throws IOException,InterruptedException {
        String key=name(username);
        synchronized(this){Cached c=cache.get(key);if(c!=null&&c.expires()>System.currentTimeMillis())return c.skin();}
        JsonObject profile=get("https://api.mojang.com/users/profiles/minecraft/"+key);
        Skin skin=null;
        if(profile!=null) {
            String id=profile.get("id").getAsString();
            if(!id.matches("[a-fA-F0-9]{32}"))throw new IOException("Invalid Minecraft profile UUID");
            JsonObject session=get("https://sessionserver.mojang.com/session/minecraft/profile/"+id+"?unsigned=false");
            if(session!=null)for(JsonElement e:session.getAsJsonArray("properties")) {
                JsonObject prop=e.getAsJsonObject();
                if("textures".equals(prop.get("name").getAsString()))skin=parseSkin(prop);
            }
        }
        synchronized(this){if(cache.size()>=512&&!cache.containsKey(key))cache.remove(cache.keySet().iterator().next());cache.put(key,new Cached(skin,System.currentTimeMillis()+(skin==null?900_000L:21_600_000L)));}
        return skin;
    }
    private JsonObject get(String url) throws IOException,InterruptedException {
        HttpRequest request=HttpRequest.newBuilder(URI.create(url)).timeout(Duration.ofSeconds(5)).header("User-Agent","Rust-CustomSkins/1.0.1").GET().build();
        HttpResponse<byte[]> response=http.send(request,HttpResponse.BodyHandlers.ofByteArray());
        {
            if(response.statusCode()==404||response.statusCode()==204)return null;
            if(response.statusCode()!=200)throw new IOException("Minecraft skin API returned HTTP "+response.statusCode()+"; try again later");
            byte[] bytes=response.body();
            if(bytes.length>65536)throw new IOException("Minecraft skin API response too large");
            try{return JsonParser.parseString(new String(bytes,StandardCharsets.UTF_8)).getAsJsonObject();}
            catch(RuntimeException e){throw new IOException("Invalid Minecraft skin API response",e);}
        }
    }
}
