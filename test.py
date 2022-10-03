# DiscordChannels.py

import discord

intents = discord.Intents.default()  # デフォルトのIntentsオブジェクトを生成
client = discord.Client(intents=intents)


# 起動時処理
@client.event
async def on_ready():
    print(client.guilds)
    for guild in client.guilds:
        print(f"{guild} {guild.id}")

client.run("MTAyMDM5MzI3MTcwNjUxNzUwNQ.G6uTJm.4oYBcJUgCG4YBPUt9VukjwuktToglzdqGrHUF8")
