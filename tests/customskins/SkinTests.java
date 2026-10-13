package customskins;
import com.google.gson.*;
import com.velocitypowered.api.util.GameProfile;
import java.nio.file.*;
import java.nio.charset.StandardCharsets;
import java.util.*;

public final class SkinTests {
    private static void rejects(Runnable call){boolean rejected=false;try{call.run();}catch(RuntimeException e){rejected=true;}assert rejected;}
    public static void main(String[] args)throws Exception {
        assert SkinData.name("ExamplePlayer").equals("exampleplayer");
        for(String value:List.of("https://example.com/skin.png","../ops.json","","a b","a".repeat(17)))rejects(()->SkinData.name(value));
        Path dir=Files.createTempDirectory("custom-skins-test");
        SkinData data=new SkinData(dir);
        SkinData.Skin steve=data.defaultSkin("steve"),alex=data.defaultSkin("alex");
        assert !steve.value().equals(alex.value());
        SkinData.validateSkin(steve);SkinData.validateSkin(alex);
        String evil=Base64.getEncoder().encodeToString("{\"textures\":{\"SKIN\":{\"url\":\"https://textures.minecraft.net.evil.example/texture/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}}}".getBytes(StandardCharsets.UTF_8));
        rejects(()->SkinData.validateSkin(new SkinData.Skin(evil,"signature")));
        data.put("ProbePlayer",new SkinData.Choice(null,alex,"alex"));
        SkinData reload=new SkinData(dir);
        assert reload.choice("PROBEPLAYER").moderated().equals("alex");
        assert reload.choice("probeplayer").skin().equals(alex);
        reload.put("ProbePlayer",null);
        assert new SkinData(dir).choice("ProbePlayer")==null;
        var properties=CustomSkins.properties(List.of(new GameProfile.Property("textures","old","old"),new GameProfile.Property("isEaglerPlayer","true","")),steve);
        assert properties.size()==2;
        assert properties.stream().filter(p->p.getName().equals("textures")).count()==1;
        assert properties.get(1).getValue().equals(steve.value());
        UUID owner=UUID.randomUUID(),member=UUID.randomUUID();
        JsonArray ops=JsonParser.parseString("[{\"uuid\":\""+owner+"\",\"name\":\"owner\",\"level\":4}]").getAsJsonArray();
        assert CustomSkins.isOperator(ops,owner);
        assert !CustomSkins.isOperator(ops,member);
        ops.get(0).getAsJsonObject().addProperty("level",3);
        assert !CustomSkins.isOperator(ops,owner);
        Files.writeString(dir.resolve("skins.json"),"broken");
        boolean failed=false;try{new SkinData(dir);}catch(java.io.IOException e){failed=true;}assert failed;
        assert Files.readString(dir.resolve("skins.json")).equals("broken");
        Files.delete(dir.resolve("skins.json"));Files.delete(dir);
        System.out.println("PASS: names, signed texture validation, persistent moderation, restore, profile replacement, operator UUID checks, corrupt-store preservation");
    }
}
